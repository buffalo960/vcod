// DecompAt.java: Ghidra script that decompiles the functions at the given
// addresses, creating a function first where analysis left only a label (a
// refexport entry reached through a pointer table, for one). Output goes to
// decomp_at.c in the working directory, under ExportDecomp's markers.
//
//   analyzeHeadless <projdir> <proj> -process <binary> -noanalysis -readOnly \
//     -scriptPath tools/re -postScript DecompAt.java 4e8db0 4e8f00
//@category Analysis
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import java.io.File;
import java.io.PrintWriter;

public class DecompAt extends GhidraScript {
    @Override
    public void run() throws Exception {
        PrintWriter out = new PrintWriter(new File(System.getProperty("user.dir"), "decomp_at.c"));
        DecompInterface ifc = new DecompInterface();
        ifc.openProgram(currentProgram);
        for (String arg : getScriptArgs()) {
            Address a = toAddr(Long.parseLong(arg, 16));
            Function f = getFunctionAt(a);
            if (f == null) {
                disassemble(a);
                f = createFunction(a, null);
            }
            if (f == null) {
                out.print("// --- no function at " + a + "\n");
                continue;
            }
            DecompileResults r = ifc.decompileFunction(f, 300, monitor);
            out.print("// --- " + f.getName() + " @ " + f.getEntryPoint() + "\n");
            out.print(r.decompileCompleted() ? r.getDecompiledFunction().getC() : "// failed\n");
            out.print("\n");
        }
        out.close();
    }
}
