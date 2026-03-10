// Decompile FPGA register-related functions by tracing assertion string xrefs
// Addresses are Ghidra-rebased (ELF VA + 0x100000)
//@category RE
import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.program.model.listing.*;
import ghidra.program.model.address.*;
import ghidra.program.model.symbol.*;
import ghidra.program.model.data.*;
import ghidra.program.util.DefinedDataIterator;

import java.util.*;

public class DecompileFpgaRegisters extends GhidraScript {

    // Search for a string in the binary and return its address
    private Address findString(String target) {
        for (Data data : DefinedDataIterator.definedStrings(currentProgram)) {
            Object val = data.getValue();
            if (val != null && val.toString().contains(target)) {
                return data.getAddress();
            }
        }
        return null;
    }

    // Find all code references to a data address
    private List<Address> getCodeXrefs(Address dataAddr) {
        List<Address> results = new ArrayList<>();
        ReferenceManager rm = currentProgram.getReferenceManager();
        for (Reference ref : rm.getReferencesTo(dataAddr)) {
            if (ref.getReferenceType().isData() || ref.getReferenceType().isFlow()) {
                results.add(ref.getFromAddress());
            }
        }
        return results;
    }

    // Find function containing an address, creating one if needed
    private Function findOrCreateFunction(Address addr) {
        FunctionManager fm = currentProgram.getFunctionManager();
        Function func = fm.getFunctionContaining(addr);
        if (func != null) return func;

        // Try to create a function at the nearest function start
        CreateFunctionCmd cmd = new CreateFunctionCmd(addr);
        cmd.applyTo(currentProgram);
        return fm.getFunctionContaining(addr);
    }

    @Override
    public void run() throws Exception {
        println("=== DecompileFpgaRegisters: Tracing FPGA register functions ===");
        println("");

        // ===================================================================
        // STRATEGY 1: Find functions via assertion string xrefs
        // Each function uses __PRETTY_FUNCTION__ in assertions, which gives
        // us the function signature as a string in .rodata. Find the string,
        // find xrefs to it, and we find the function.
        // ===================================================================

        // Function signatures to search for (assertion __PRETTY_FUNCTION__ strings)
        String[][] signatureTargets = new String[][] {
            // CalculateDigitalConfigurationAddress functions
            {"CalculateDigitalConfigurationAddress_DigitalDerived(int)", "CalcDigAddr_DigitalDerived"},
            {"CalculateDigitalConfigurationAddress_SmartCable(int, int)", "CalcDigAddr_SmartCable"},

            // MsoFpga register-writing functions
            {"MsoFpga::SetAnalogChannelConfiguration", "SetAnalogChannelConfiguration"},
            {"MsoFpga::SetAnalogTriggerConfiguration", "SetAnalogTriggerConfiguration"},
            {"MsoFpga::InternalSetDigitalConfiguration", "InternalSetDigitalConfiguration"},
            {"MsoFpga::SetTestDataConfiguration", "SetTestDataConfiguration"},
            {"MsoFpga::SetEnabledSmartCableDigitalChannels", "SetEnabledSmartCableDigitalChannels"},
            {"MsoFpga::ResetJesdPhyTransceivers", "ResetJesdPhyTransceivers"},
            {"MsoFpga::WriteSpi(", "WriteSpi"},
            {"MsoFpga::ReadSpi_7BitAddress", "ReadSpi_7BitAddress"},
            {"MsoFpga::ReadSpi_15BitAddress", "ReadSpi_15BitAddress"},

            // Logic2FpgaDevice functions
            {"Logic2FpgaDevice::WriteAd9637Register", "WriteAd9637Register"},
            {"Logic2FpgaDevice::SetCaptureParameters", "SetCaptureParameters"},
            {"Logic2FpgaDevice::SetAd9637Enabled", "SetAd9637Enabled"},
            {"Logic2FpgaDevice::InitHmcad1100", "InitHmcad1100"},
            {"Logic2FpgaDevice::SetTestPattenHmcad1100", "SetTestPattenHmcad1100"},

            // SetField instantiations (bit-field layout info)
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = Saleae::MsoFpga::TriggerType", "SetField_TriggerType"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = Saleae::MsoFpga::TriggerMode", "SetField_TriggerMode"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = Saleae::DownsampleMode", "SetField_DownsampleMode"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = Saleae::MsoFpga::SpiClockEdge", "SetField_SpiClockEdge"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = Saleae::MsoFpga::SpiWireConfiguration", "SetField_SpiWireConfiguration"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = Saleae::MsoFpga::SpiAddressDataFormat", "SetField_SpiAddressDataFormat"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = unsigned char", "SetField_uint_uchar"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = unsigned int", "SetField_uint_uint"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = unsigned short", "SetField_uint_ushort"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned int, S = bool", "SetField_uint_bool"},
            {"SetField(T &, S, uint64_t, uint64_t) [T = unsigned char, S = bool", "SetField_uchar_bool"},

            // SetDigitalTestPattern
            {"SetDigitalTestPattern(Saleae::MsoFpga", "SetDigitalTestPattern"},

            // Voltage threshold
            {"SetDigitalDerivedVoltageThresholds", "SetDigitalDerivedVoltageThresholds_ref"},

            // StartSampling (various device types)
            {"LogicProfessionalDevice::StartSampling", "LogicProfessionalDevice_StartSampling"},
            {"LogicGraduateDevice::StartSampling", "LogicGraduateDevice_StartSampling"},

            // PLL locking
            {"PLL(s) failed to lock", "PLL_FailedToLock_ref"},
            {"bank enabled, PLL is locked", "PLL_Locked_ref"},
        };

        // ===================================================================
        // STRATEGY 2: Known ELF VAs (+0x100000 for Ghidra rebase)
        // These are from MapFpgaVtable.py and earlier analysis
        // ===================================================================

        String[][] knownAddresses = new String[][] {
            // FpgaDeviceFeatures vtable methods (ELF VA + 0x100000)
            {"01d535f0", "FpgaDeviceFeatures_WriteRegisters"},     // ELF 0x1c535f0
            {"01d53990", "FpgaDeviceFeatures_ReadRegister"},       // ELF 0x1c53990
            {"01d53af0", "FpgaDeviceFeatures_ReadRegisters"},      // ELF 0x1c53af0
            {"01d53e30", "FpgaDeviceFeatures_WriteRegisterBit"},   // ELF 0x1c53e30

            // ProFpgaDeviceFeatures override
            {"01d549d0", "ProFpgaDeviceFeatures_WriteRegisters"},  // ELF 0x1c549d0

            // WriteConfigurationData
            {"01d544b0", "FpgaDeviceFeatures_WriteConfigurationData"}, // ELF 0x1c544b0

            // SetVoltageLevel (LogicProfessionalDevice)
            // from string at 0x3fc5b5
            // Need to find its code VA via xref
        };

        // ===================================================================
        // Collect all functions to decompile
        // ===================================================================
        Map<String, Function> toDecompile = new LinkedHashMap<>();

        // Process signature-based targets
        println("--- Searching for functions via assertion string xrefs ---");
        for (String[] target : signatureTargets) {
            String searchStr = target[0];
            String label = target[1];

            Address strAddr = findString(searchStr);
            if (strAddr == null) {
                println("  STRING NOT FOUND: " + searchStr);
                continue;
            }

            List<Address> xrefs = getCodeXrefs(strAddr);
            if (xrefs.isEmpty()) {
                println("  NO XREFS for: " + searchStr + " @ " + strAddr);
                continue;
            }

            for (Address xref : xrefs) {
                Function func = findOrCreateFunction(xref);
                if (func != null) {
                    String key = func.getEntryPoint().toString();
                    if (!toDecompile.containsKey(key)) {
                        toDecompile.put(key, func);
                        println("  FOUND: " + label + " -> " + func.getName()
                            + " @ " + func.getEntryPoint() + " (via xref at " + xref + ")");
                    }
                } else {
                    println("  XREF but no function: " + label + " at " + xref);
                }
            }
        }

        // Process known-address targets
        println("");
        println("--- Decompiling functions at known addresses ---");
        AddressFactory af = currentProgram.getAddressFactory();
        FunctionManager fm = currentProgram.getFunctionManager();

        for (String[] target : knownAddresses) {
            Address addr = af.getAddress(target[0]);
            Function func = fm.getFunctionContaining(addr);
            if (func == null) {
                func = fm.getFunctionAt(addr);
            }
            if (func != null) {
                String key = func.getEntryPoint().toString();
                if (!toDecompile.containsKey(key)) {
                    toDecompile.put(key, func);
                    println("  FOUND: " + target[1] + " -> " + func.getName()
                        + " @ " + func.getEntryPoint());
                }
            } else {
                println("  NOT FOUND: " + target[1] + " at " + target[0]);
            }
        }

        // ===================================================================
        // Also search for the FPGA WriteRegister format string xrefs to
        // find ALL functions that write FPGA registers
        // ===================================================================
        println("");
        println("--- Finding ALL FPGA register write sites ---");
        Address writeFormatAddr = findString("FPGA WriteRegister 0x{:04x} = 0x{:08x}");
        if (writeFormatAddr != null) {
            List<Address> writeXrefs = getCodeXrefs(writeFormatAddr);
            println("  Found " + writeXrefs.size() + " WriteRegister format string xrefs");
            Set<String> writeFuncs = new LinkedHashSet<>();
            for (Address xref : writeXrefs) {
                Function func = findOrCreateFunction(xref);
                if (func != null) {
                    String key = func.getEntryPoint().toString();
                    writeFuncs.add(key);
                    if (!toDecompile.containsKey(key)) {
                        toDecompile.put(key, func);
                    }
                }
            }
            for (String fk : writeFuncs) {
                Function f = toDecompile.get(fk);
                println("  WriteRegister caller: " + f.getName() + " @ " + f.getEntryPoint());
            }
        }

        Address readFormatAddr = findString("FPGA ReadRegister 0x{:04X} = 0x{:08X}");
        if (readFormatAddr != null) {
            List<Address> readXrefs = getCodeXrefs(readFormatAddr);
            println("  Found " + readXrefs.size() + " ReadRegister format string xrefs");
            for (Address xref : readXrefs) {
                Function func = findOrCreateFunction(xref);
                if (func != null) {
                    String key = func.getEntryPoint().toString();
                    if (!toDecompile.containsKey(key)) {
                        toDecompile.put(key, func);
                        println("  ReadRegister caller: " + func.getName() + " @ " + func.getEntryPoint());
                    }
                }
            }
        }

        // ===================================================================
        // Decompile everything
        // ===================================================================
        println("");
        println("=== DECOMPILING " + toDecompile.size() + " FUNCTIONS ===");
        println("");

        DecompInterface decomp = new DecompInterface();
        decomp.openProgram(currentProgram);

        for (Map.Entry<String, Function> entry : toDecompile.entrySet()) {
            Function func = entry.getValue();
            println("=== " + func.getName() + " @ " + func.getEntryPoint()
                + " (size: " + func.getBody().getNumAddresses() + " bytes) ===");

            DecompileResults res = decomp.decompileFunction(func, 300, monitor);
            if (res.decompileCompleted()) {
                String cCode = res.getDecompiledFunction().getC();
                println(cCode);
            } else {
                println("DECOMPILE FAILED: " + res.getErrorMessage());
            }
            println("");
        }

        decomp.dispose();
        println("=== DecompileFpgaRegisters complete ===");
    }
}
