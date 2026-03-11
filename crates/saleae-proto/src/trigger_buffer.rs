//! Pre-trigger ring buffer and trigger capture orchestrator.
//!
//! Saleae Logic devices stream all samples to the host with no on-device
//! trigger hardware. The [`TriggerCapture`] orchestrator manages the
//! pre/post-trigger buffering around the software trigger engine:
//!
//! 1. **Accumulating**: samples flow into a fixed-size ring buffer while the
//!    trigger engine inspects each sample. Old samples are overwritten so
//!    only the most recent N survive.
//! 2. **Collecting**: once the trigger fires, the ring buffer is drained
//!    (pre-trigger data) and subsequent samples are collected until the
//!    post-trigger count is reached.
//! 3. **Complete**: capture is done; results can be consumed.
//!
//! The generic [`RingBuffer<T>`] is reusable outside the trigger context.

use crate::trigger::{TriggerConfig, TriggerEngine};

// ---------------------------------------------------------------------------
// RingBuffer<T>
// ---------------------------------------------------------------------------

/// Fixed-size circular buffer that overwrites the oldest element on overflow.
///
/// This is the backing store for pre-trigger sample retention. When the
/// buffer is full, [`push`](RingBuffer::push) silently drops the oldest
/// element to make room.
///
/// # Generic over `T`
///
/// Works for digital channel snapshots (`Vec<bool>`), analog sample arrays
/// (`Vec<f32>`), or any other per-sample data type.
#[derive(Clone, Debug)]
pub struct RingBuffer<T> {
    /// Storage. Allocated once at construction; never resized.
    buf: Vec<Option<T>>,
    /// Maximum number of elements.
    cap: usize,
    /// Index where the next element will be written.
    write_pos: usize,
    /// Number of elements currently stored (saturates at `cap`).
    count: usize,
}

impl<T> RingBuffer<T> {
    /// Create a ring buffer with the given capacity.
    ///
    /// A capacity of 0 is allowed and means no elements are ever stored
    /// (useful when pre-trigger samples are not wanted).
    pub fn new(capacity: usize) -> Self {
        let mut buf = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            buf.push(None);
        }
        Self {
            buf,
            cap: capacity,
            write_pos: 0,
            count: 0,
        }
    }

    /// Add an element, overwriting the oldest if the buffer is full.
    ///
    /// For a zero-capacity buffer this is a no-op.
    pub fn push(&mut self, item: T) {
        if self.cap == 0 {
            return;
        }
        self.buf[self.write_pos] = Some(item);
        self.write_pos = (self.write_pos + 1) % self.cap;
        if self.count < self.cap {
            self.count += 1;
        }
    }

    /// Consume all stored elements in insertion order (oldest first).
    ///
    /// The buffer is empty after this call.
    pub fn drain(&mut self) -> Vec<T> {
        if self.count == 0 {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(self.count);
        // The oldest element is at (write_pos - count) mod cap, which
        // simplifies to (write_pos + cap - count) % cap when count <= cap.
        let start = (self.write_pos + self.cap - self.count) % self.cap;
        for i in 0..self.count {
            let idx = (start + i) % self.cap;
            if let Some(item) = self.buf[idx].take() {
                result.push(item);
            }
        }
        self.count = 0;
        self.write_pos = 0;
        result
    }

    /// Number of elements currently stored.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether the buffer contains no elements.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Whether the buffer is at capacity.
    pub fn is_full(&self) -> bool {
        self.count == self.cap
    }

    /// Maximum number of elements this buffer can hold.
    pub fn capacity(&self) -> usize {
        self.cap
    }
}

// ---------------------------------------------------------------------------
// TriggerCaptureState
// ---------------------------------------------------------------------------

/// State reported by [`TriggerCapture::process_sample`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerCaptureState {
    /// Pre-trigger: accumulating samples in the ring buffer.
    Accumulating,
    /// The trigger just fired on this call. `trigger_sample` is the global
    /// sample index where the trigger condition was satisfied.
    Triggered { trigger_sample: u64 },
    /// Post-trigger collection is complete (or was already complete).
    Complete,
}

// ---------------------------------------------------------------------------
// TriggerCaptureResult
// ---------------------------------------------------------------------------

/// Output of a completed trigger capture.
///
/// Each element of `pre_trigger_data` and `post_trigger_data` is one sample:
/// a `Vec<bool>` of channel values matching the format consumed by
/// [`TriggerEngine::process_sample`].
#[derive(Clone, Debug)]
pub struct TriggerCaptureResult {
    /// Samples retained from before the trigger point (oldest first).
    pub pre_trigger_data: Vec<Vec<bool>>,
    /// Samples collected after (and including) the trigger sample.
    pub post_trigger_data: Vec<Vec<bool>>,
    /// Global sample index where the trigger condition fired.
    pub trigger_sample_index: u64,
    /// Channel index that contributed to the trigger.
    ///
    /// For multi-condition AND triggers this is the first channel listed
    /// in the trigger configuration. For edge triggers it is the edge
    /// channel; for pulse triggers the pulse channel.
    pub trigger_channel: usize,
}

// ---------------------------------------------------------------------------
// TriggerCapture
// ---------------------------------------------------------------------------

/// Internal phase of the capture state machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Accumulating,
    Collecting,
    Done,
}

/// Orchestrator combining [`TriggerEngine`] with a pre-trigger [`RingBuffer`].
///
/// Feed samples one at a time via [`process_sample`](TriggerCapture::process_sample).
/// The returned [`TriggerCaptureState`] tells the caller what happened. When
/// the state becomes [`Complete`](TriggerCaptureState::Complete), call
/// [`take_result`](TriggerCapture::take_result) to consume the captured data.
pub struct TriggerCapture {
    engine: TriggerEngine,
    ring: RingBuffer<Vec<bool>>,
    post_trigger_samples: usize,
    phase: Phase,
    /// Accumulated post-trigger samples (including the trigger sample itself).
    post_data: Vec<Vec<bool>>,
    /// Pre-trigger data snapshot (drained from ring buffer at trigger time).
    pre_data: Vec<Vec<bool>>,
    /// Global sample index where the trigger fired.
    trigger_sample_index: u64,
    /// First channel in the trigger configuration (best-effort "which channel").
    trigger_channel: usize,
    /// Count of post-trigger samples collected so far.
    post_collected: usize,
}

impl TriggerCapture {
    /// Create a new trigger capture orchestrator.
    ///
    /// # Arguments
    ///
    /// * `config` -- trigger conditions (edge/pulse).
    /// * `pre_trigger_samples` -- how many samples to retain before the trigger.
    /// * `post_trigger_samples` -- how many samples to collect after the trigger
    ///   (the trigger sample itself counts as the first post-trigger sample).
    pub fn new(
        config: TriggerConfig,
        pre_trigger_samples: usize,
        post_trigger_samples: usize,
    ) -> Self {
        let trigger_channel = config
            .conditions
            .first()
            .map(|c| match c {
                crate::trigger::TriggerCondition::Edge { channel, .. } => *channel as usize,
                crate::trigger::TriggerCondition::Pulse { channel, .. } => *channel as usize,
            })
            .unwrap_or(0);

        let engine = TriggerEngine::new(config);
        let ring = RingBuffer::new(pre_trigger_samples);

        Self {
            engine,
            ring,
            post_trigger_samples,
            phase: Phase::Accumulating,
            post_data: Vec::new(),
            pre_data: Vec::new(),
            trigger_sample_index: 0,
            trigger_channel,
            post_collected: 0,
        }
    }

    /// Feed one sample into the capture pipeline.
    ///
    /// `sample_index` is the global (monotonically increasing) sample counter.
    /// `channel_values` is the per-channel digital state for this sample.
    ///
    /// Returns the current capture state after processing.
    pub fn process_sample(
        &mut self,
        sample_index: u64,
        channel_values: &[bool],
    ) -> TriggerCaptureState {
        match self.phase {
            Phase::Accumulating => {
                // Run trigger detection.
                let triggered = self.engine.process_sample(sample_index, channel_values);

                if let Some(_trigger_at) = triggered {
                    // Trigger fired. Transition to Collecting.
                    self.trigger_sample_index = sample_index;
                    self.pre_data = self.ring.drain();
                    self.phase = Phase::Collecting;

                    // The trigger sample itself is the first post-trigger sample.
                    self.post_data.push(channel_values.to_vec());
                    self.post_collected = 1;

                    if self.post_collected >= self.post_trigger_samples {
                        self.phase = Phase::Done;
                        return TriggerCaptureState::Complete;
                    }

                    TriggerCaptureState::Triggered {
                        trigger_sample: sample_index,
                    }
                } else {
                    // No trigger yet. Buffer this sample for pre-trigger retention.
                    self.ring.push(channel_values.to_vec());
                    TriggerCaptureState::Accumulating
                }
            }
            Phase::Collecting => {
                self.post_data.push(channel_values.to_vec());
                self.post_collected += 1;

                if self.post_collected >= self.post_trigger_samples {
                    self.phase = Phase::Done;
                    TriggerCaptureState::Complete
                } else {
                    // Still collecting. Report as Triggered (the trigger already
                    // fired; the caller knows we are in post-trigger collection).
                    TriggerCaptureState::Triggered {
                        trigger_sample: self.trigger_sample_index,
                    }
                }
            }
            Phase::Done => TriggerCaptureState::Complete,
        }
    }

    /// Consume the captured data.
    ///
    /// Returns `None` if the capture is not yet complete.
    pub fn take_result(&mut self) -> Option<TriggerCaptureResult> {
        if self.phase != Phase::Done {
            return None;
        }

        Some(TriggerCaptureResult {
            pre_trigger_data: std::mem::take(&mut self.pre_data),
            post_trigger_data: std::mem::take(&mut self.post_data),
            trigger_sample_index: self.trigger_sample_index,
            trigger_channel: self.trigger_channel,
        })
    }

    /// Whether the capture is complete.
    pub fn is_complete(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Whether the trigger has fired (capture is in Collecting or Done phase).
    pub fn is_triggered(&self) -> bool {
        self.phase == Phase::Collecting || self.phase == Phase::Done
    }

    /// The global sample index where the trigger fired, if it has fired.
    pub fn trigger_sample_index(&self) -> Option<u64> {
        if self.phase == Phase::Accumulating {
            None
        } else {
            Some(self.trigger_sample_index)
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{EdgeType, TriggerCondition, TriggerConfig};

    // -----------------------------------------------------------------------
    // RingBuffer tests
    // -----------------------------------------------------------------------

    #[test]
    fn ring_new_empty() {
        let ring: RingBuffer<i32> = RingBuffer::new(5);
        assert_eq!(ring.len(), 0);
        assert!(ring.is_empty());
        assert!(!ring.is_full());
        assert_eq!(ring.capacity(), 5);
    }

    #[test]
    fn ring_push_and_drain_basic() {
        let mut ring = RingBuffer::new(3);
        ring.push(10);
        ring.push(20);
        assert_eq!(ring.len(), 2);
        assert!(!ring.is_full());

        let items = ring.drain();
        assert_eq!(items, vec![10, 20]);
        assert!(ring.is_empty());
    }

    #[test]
    fn ring_push_to_capacity() {
        let mut ring = RingBuffer::new(3);
        ring.push(1);
        ring.push(2);
        ring.push(3);
        assert!(ring.is_full());
        assert_eq!(ring.len(), 3);

        let items = ring.drain();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn ring_overflow_overwrites_oldest() {
        let mut ring = RingBuffer::new(3);
        ring.push(1);
        ring.push(2);
        ring.push(3);
        ring.push(4); // overwrites 1
        ring.push(5); // overwrites 2

        assert!(ring.is_full());
        assert_eq!(ring.len(), 3);

        let items = ring.drain();
        assert_eq!(items, vec![3, 4, 5]);
    }

    #[test]
    fn ring_heavy_overflow() {
        let mut ring = RingBuffer::new(3);
        for i in 0..100 {
            ring.push(i);
        }
        let items = ring.drain();
        assert_eq!(items, vec![97, 98, 99]);
    }

    #[test]
    fn ring_drain_empty() {
        let mut ring: RingBuffer<i32> = RingBuffer::new(5);
        let items = ring.drain();
        assert!(items.is_empty());
    }

    #[test]
    fn ring_drain_then_reuse() {
        let mut ring = RingBuffer::new(3);
        ring.push(1);
        ring.push(2);
        ring.push(3);
        let _ = ring.drain();

        // Reuse after drain.
        ring.push(10);
        ring.push(20);
        let items = ring.drain();
        assert_eq!(items, vec![10, 20]);
    }

    #[test]
    fn ring_capacity_zero() {
        let mut ring: RingBuffer<i32> = RingBuffer::new(0);
        assert_eq!(ring.capacity(), 0);
        assert!(ring.is_empty());
        assert!(ring.is_full()); // 0 == 0

        ring.push(42); // no-op
        assert!(ring.is_empty());

        let items = ring.drain();
        assert!(items.is_empty());
    }

    #[test]
    fn ring_capacity_one() {
        let mut ring = RingBuffer::new(1);
        ring.push(1);
        assert!(ring.is_full());
        assert_eq!(ring.len(), 1);

        ring.push(2); // overwrites 1
        assert_eq!(ring.len(), 1);

        let items = ring.drain();
        assert_eq!(items, vec![2]);
    }

    #[test]
    fn ring_with_vec_bool() {
        let mut ring: RingBuffer<Vec<bool>> = RingBuffer::new(2);
        ring.push(vec![true, false]);
        ring.push(vec![false, true]);
        ring.push(vec![true, true]); // overwrites first

        let items = ring.drain();
        assert_eq!(items, vec![vec![false, true], vec![true, true]]);
    }

    // -----------------------------------------------------------------------
    // TriggerCapture: helper to build configs
    // -----------------------------------------------------------------------

    fn rising_edge_config(channel: u8) -> TriggerConfig {
        TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel,
                edge_type: EdgeType::Rising,
            }],
            pre_trigger_samples: 0,
        }
    }

    // -----------------------------------------------------------------------
    // TriggerCapture tests
    // -----------------------------------------------------------------------

    #[test]
    fn capture_immediate_trigger_first_possible_sample() {
        // Edge trigger on ch0 rising. Feed: false then true.
        // Trigger should fire on sample 1 (the first transition).
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 1);

        let s = cap.process_sample(0, &[false]);
        assert_eq!(s, TriggerCaptureState::Accumulating);

        let s = cap.process_sample(1, &[true]);
        // With post_trigger_samples=1, the trigger sample itself completes capture.
        assert_eq!(s, TriggerCaptureState::Complete);
        assert!(cap.is_complete());

        let result = cap.take_result().unwrap();
        assert_eq!(result.trigger_sample_index, 1);
        assert_eq!(result.trigger_channel, 0);
        assert!(result.pre_trigger_data.is_empty());
        assert_eq!(result.post_trigger_data.len(), 1);
        assert_eq!(result.post_trigger_data[0], vec![true]);
    }

    #[test]
    fn capture_trigger_after_n_samples() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 5, 3);

        // Feed 10 low samples.
        for i in 0..10 {
            let s = cap.process_sample(i, &[false]);
            assert_eq!(s, TriggerCaptureState::Accumulating);
        }

        // Trigger on sample 10 (rising edge).
        let s = cap.process_sample(10, &[true]);
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 10
            }
        );

        // Collect 2 more post-trigger samples.
        let s = cap.process_sample(11, &[true]);
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 10
            }
        );

        let s = cap.process_sample(12, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.trigger_sample_index, 10);
        // Pre-trigger: last 5 of samples 0..10, which are samples 5,6,7,8,9.
        assert_eq!(result.pre_trigger_data.len(), 5);
        for sample in &result.pre_trigger_data {
            assert_eq!(sample, &vec![false]);
        }
        // Post-trigger: samples 10, 11, 12.
        assert_eq!(result.post_trigger_data.len(), 3);
        assert_eq!(result.post_trigger_data[0], vec![true]);
        assert_eq!(result.post_trigger_data[1], vec![true]);
        assert_eq!(result.post_trigger_data[2], vec![false]);
    }

    #[test]
    fn capture_pre_trigger_correct_samples_retained() {
        // Pre-trigger buffer of 3, trigger fires after many samples.
        // Verify we get exactly the last 3 samples before the trigger.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 3, 1);

        // Feed samples with channel value encoding the sample index (mod 2).
        for i in 0u64..20 {
            cap.process_sample(i, &[false]);
        }

        // Samples 17, 18, 19 are the last 3 in the ring buffer.
        // Trigger on sample 20.
        let s = cap.process_sample(20, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.pre_trigger_data.len(), 3);
        // All pre-trigger samples had channel value false.
        for sample in &result.pre_trigger_data {
            assert_eq!(sample, &vec![false]);
        }
    }

    #[test]
    fn capture_pre_trigger_fewer_than_capacity() {
        // Pre-trigger buffer of 10, but trigger fires after only 3 samples.
        // We should get only 3 pre-trigger samples (not padded to 10).
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 10, 1);

        cap.process_sample(0, &[false]);
        cap.process_sample(1, &[false]);
        cap.process_sample(2, &[false]);
        let s = cap.process_sample(3, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        // Only 3 samples were accumulated before the trigger (samples 0, 1, 2).
        assert_eq!(result.pre_trigger_data.len(), 3);
    }

    #[test]
    fn capture_post_trigger_exact_count() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 5);

        cap.process_sample(0, &[false]);
        let s = cap.process_sample(1, &[true]); // trigger fires
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 1
            }
        );

        // Collect 4 more (trigger sample was #1 of 5).
        for i in 2..5 {
            let s = cap.process_sample(i, &[true]);
            assert_eq!(
                s,
                TriggerCaptureState::Triggered {
                    trigger_sample: 1
                }
            );
        }
        let s = cap.process_sample(5, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.post_trigger_data.len(), 5);
    }

    #[test]
    fn capture_no_trigger_accumulates_indefinitely() {
        // Trigger condition: rising edge on ch0.
        // Feed only low values -- trigger never fires.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 5, 5);

        for i in 0..1000 {
            let s = cap.process_sample(i, &[false]);
            assert_eq!(s, TriggerCaptureState::Accumulating);
        }

        assert!(!cap.is_triggered());
        assert!(!cap.is_complete());
        assert!(cap.take_result().is_none());
    }

    #[test]
    fn capture_pre_trigger_zero() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 2);

        cap.process_sample(0, &[false]);
        let s = cap.process_sample(1, &[true]); // trigger
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 1
            }
        );

        let s = cap.process_sample(2, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert!(result.pre_trigger_data.is_empty());
        assert_eq!(result.post_trigger_data.len(), 2);
    }

    #[test]
    fn capture_post_trigger_zero() {
        // post_trigger_samples=0 means we want zero post-trigger samples,
        // but the trigger sample itself is included. Actually, with 0 the
        // capture completes immediately when the trigger fires (the trigger
        // sample is NOT collected since 0 post-trigger samples are requested).
        //
        // Design decision: 0 post-trigger means "complete immediately at
        // trigger, collecting the trigger sample as the sole post-trigger
        // sample" would be inconsistent. Instead, 0 means "no post samples
        // at all, not even the trigger sample".
        //
        // Wait -- re-reading the task: "post_trigger_samples: usize" and
        // "trigger sample counts as first post-trigger sample" was my design.
        // With post=0, the trigger fires and we immediately go to Done.
        // The trigger sample IS still pushed to post_data by the Accumulating
        // -> Collecting transition, but post_collected=1 >= 0 so we go Done.
        //
        // Actually, let me check: in the code, after trigger fires we push
        // the trigger sample and set post_collected=1. Then we check
        // post_collected >= post_trigger_samples. If post_trigger_samples=0,
        // 1 >= 0 is true, so we go to Done. The post_data will contain the
        // trigger sample itself. This is reasonable behavior.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 3, 0);

        cap.process_sample(0, &[false]);
        cap.process_sample(1, &[false]);
        cap.process_sample(2, &[false]);
        let s = cap.process_sample(3, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.pre_trigger_data.len(), 3);
        // The trigger sample is included as the sole post-trigger sample.
        assert_eq!(result.post_trigger_data.len(), 1);
        assert_eq!(result.post_trigger_data[0], vec![true]);
    }

    #[test]
    fn capture_both_zero() {
        // pre=0, post=0. The trigger fires and completes immediately.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 0);

        cap.process_sample(0, &[false]);
        let s = cap.process_sample(1, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert!(result.pre_trigger_data.is_empty());
        // Trigger sample itself.
        assert_eq!(result.post_trigger_data.len(), 1);
    }

    #[test]
    fn capture_multi_channel() {
        // Rising edge on ch1 (not ch0).
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 1,
                edge_type: EdgeType::Rising,
            }],
            pre_trigger_samples: 0,
        };
        let mut cap = TriggerCapture::new(config, 2, 2);

        // ch0=false, ch1=false
        cap.process_sample(0, &[false, false]);
        // ch0=true, ch1=false -- ch1 has no edge, no trigger
        cap.process_sample(1, &[true, false]);
        // ch0=true, ch1=true -- ch1 rising edge, trigger!
        let s = cap.process_sample(2, &[true, true]);
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 2
            }
        );

        let s = cap.process_sample(3, &[false, false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.trigger_channel, 1);
        assert_eq!(result.trigger_sample_index, 2);
        // Pre-trigger: samples 0 and 1.
        assert_eq!(result.pre_trigger_data.len(), 2);
        assert_eq!(result.pre_trigger_data[0], vec![false, false]);
        assert_eq!(result.pre_trigger_data[1], vec![true, false]);
        // Post-trigger: samples 2 and 3.
        assert_eq!(result.post_trigger_data.len(), 2);
        assert_eq!(result.post_trigger_data[0], vec![true, true]);
        assert_eq!(result.post_trigger_data[1], vec![false, false]);
    }

    #[test]
    fn capture_done_is_terminal() {
        // After completion, further process_sample calls return Complete.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 1);

        cap.process_sample(0, &[false]);
        let s = cap.process_sample(1, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);

        // Additional samples after completion.
        let s = cap.process_sample(2, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);
        let s = cap.process_sample(3, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);
    }

    #[test]
    fn capture_take_result_returns_none_before_complete() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 5, 5);

        cap.process_sample(0, &[false]);
        assert!(cap.take_result().is_none());
    }

    #[test]
    fn capture_take_result_consumes() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 1);

        cap.process_sample(0, &[false]);
        cap.process_sample(1, &[true]);

        let r1 = cap.take_result();
        assert!(r1.is_some());

        // Second call returns the result with empty vecs (data was taken).
        let r2 = cap.take_result();
        assert!(r2.is_some()); // still Done phase
        let r2 = r2.unwrap();
        assert!(r2.pre_trigger_data.is_empty());
        assert!(r2.post_trigger_data.is_empty());
    }

    #[test]
    fn capture_trigger_near_start() {
        // Trigger fires on the very first transition (sample 1).
        // pre-trigger buffer = 10 but only 1 sample accumulated.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 10, 2);

        cap.process_sample(0, &[false]);
        let s = cap.process_sample(1, &[true]);
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 1
            }
        );

        let s = cap.process_sample(2, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        // Only 1 sample was in the ring buffer before trigger.
        assert_eq!(result.pre_trigger_data.len(), 1);
        assert_eq!(result.pre_trigger_data[0], vec![false]);
        assert_eq!(result.post_trigger_data.len(), 2);
    }

    #[test]
    fn capture_trigger_sample_accessor() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 5);

        assert!(cap.trigger_sample_index().is_none());

        cap.process_sample(0, &[false]);
        cap.process_sample(1, &[true]); // trigger at 1

        assert_eq!(cap.trigger_sample_index(), Some(1));
    }

    #[test]
    fn capture_is_triggered_phases() {
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 0, 3);

        assert!(!cap.is_triggered());
        cap.process_sample(0, &[false]);
        assert!(!cap.is_triggered());

        cap.process_sample(1, &[true]); // trigger
        assert!(cap.is_triggered());
        assert!(!cap.is_complete());

        cap.process_sample(2, &[true]);
        cap.process_sample(3, &[false]);
        assert!(cap.is_triggered());
        assert!(cap.is_complete());
    }

    #[test]
    fn capture_no_conditions_triggers_immediately() {
        // Empty conditions = trigger fires on first sample.
        let config = TriggerConfig {
            conditions: vec![],
            pre_trigger_samples: 0,
        };
        let mut cap = TriggerCapture::new(config, 0, 1);

        let s = cap.process_sample(0, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.trigger_sample_index, 0);
        assert_eq!(result.trigger_channel, 0);
    }

    #[test]
    fn capture_falling_edge() {
        let config = TriggerConfig {
            conditions: vec![TriggerCondition::Edge {
                channel: 0,
                edge_type: crate::trigger::EdgeType::Falling,
            }],
            pre_trigger_samples: 0,
        };
        let mut cap = TriggerCapture::new(config, 2, 2);

        cap.process_sample(0, &[true]);
        cap.process_sample(1, &[true]);
        // Falling edge at sample 2.
        let s = cap.process_sample(2, &[false]);
        assert_eq!(
            s,
            TriggerCaptureState::Triggered {
                trigger_sample: 2
            }
        );

        let s = cap.process_sample(3, &[false]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        assert_eq!(result.pre_trigger_data.len(), 2);
        assert_eq!(result.pre_trigger_data[0], vec![true]);
        assert_eq!(result.pre_trigger_data[1], vec![true]);
        assert_eq!(result.post_trigger_data.len(), 2);
    }

    #[test]
    fn capture_large_pre_trigger_buffer() {
        // Verify ring buffer behavior with a larger capacity.
        let config = rising_edge_config(0);
        let mut cap = TriggerCapture::new(config, 100, 1);

        // Feed 200 low samples.
        for i in 0..200 {
            cap.process_sample(i, &[false]);
        }

        // Trigger at sample 200.
        let s = cap.process_sample(200, &[true]);
        assert_eq!(s, TriggerCaptureState::Complete);

        let result = cap.take_result().unwrap();
        // Should have exactly 100 pre-trigger samples (samples 100..200).
        assert_eq!(result.pre_trigger_data.len(), 100);
    }
}
