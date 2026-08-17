import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.Reference;
import ghidra.program.model.symbol.ReferenceManager;

import java.io.File;
import java.io.PrintWriter;
import java.util.ArrayList;
import java.util.List;

public class ExportAll extends GhidraScript {

    private String safe(String s) {
        return s.replaceAll("[^A-Za-z0-9_.-]", "_");
    }

    @Override
    public void run() throws Exception {
        String outDir = getScriptArgs().length > 0 ? getScriptArgs()[0] : "build";
        File decompDir = new File(outDir, "decomp");
        decompDir.mkdirs();

        DecompInterface di = new DecompInterface();
        di.openProgram(currentProgram);

        FunctionManager fm = currentProgram.getFunctionManager();
        ReferenceManager rm = currentProgram.getReferenceManager();

        List<String> json = new ArrayList<>();

        for (Function f : fm.getFunctions(true)) {
            String name = f.getName();
            Address entry = f.getEntryPoint();

            DecompileResults res = di.decompileFunction(f, 60, monitor);
            String c = res.decompileCompleted()
                    ? res.getDecompiledFunction().getC()
                    : "// DECOMPILATION FAILED: " + res.getErrorMessage() + "\n";

            File out = new File(decompDir, safe(name) + "_" + entry.toString().replace(":", "_") + ".c");
            try (PrintWriter pw = new PrintWriter(out, "UTF-8")) {
                pw.print(c);
            }

            List<String> callers = new ArrayList<>();
            for (Reference r : rm.getReferencesTo(entry)) {
                Function cf = fm.getFunctionContaining(r.getFromAddress());
                if (cf != null && !callers.contains(cf.getName())) callers.add(cf.getName());
            }

            List<String> callees = new ArrayList<>();
            for (Function cf : f.getCalledFunctions(monitor)) {
                if (!callees.contains(cf.getName())) callees.add(cf.getName());
            }

            StringBuilder sb = new StringBuilder();
            sb.append("  {\"name\": \"").append(name).append("\"");
            sb.append(", \"entry\": \"").append(entry).append("\"");
            sb.append(", \"size\": ").append(f.getBody().getNumAddresses());
            sb.append(", \"called_by\": [").append(quoteJoin(callers)).append("]");
            sb.append(", \"calls\": [").append(quoteJoin(callees)).append("]}");
            json.add(sb.toString());
        }

        File jf = new File(outDir, "functions.json");
        try (PrintWriter pw = new PrintWriter(jf, "UTF-8")) {
            pw.println("[");
            pw.println(String.join(",\n", json));
            pw.println("]");
        }

        println("EXPORTED functions=" + json.size() + " to " + outDir);
        di.dispose();
    }

    private String quoteJoin(List<String> xs) {
        List<String> q = new ArrayList<>();
        for (String x : xs) q.add("\"" + x + "\"");
        return String.join(", ", q);
    }
}
