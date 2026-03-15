import com.sun.source.tree.CompilationUnitTree;
import com.sun.source.util.JavacTask;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.stream.Collectors;
import javax.tools.Diagnostic;
import javax.tools.DiagnosticCollector;
import javax.tools.JavaCompiler;
import javax.tools.JavaFileObject;
import javax.tools.StandardJavaFileManager;
import javax.tools.ToolProvider;

/**
 * Generates aggregated OpenJDK diagnostics for invalid verification fixtures.
 *
 * <p>This uses {@link JavacTask#parse()} and {@link JavacTask#analyze()} instead of
 * {@link JavaCompiler.CompilationTask#call()} because the verification suite needs type-check
 * diagnostics even when other files in the same batch have parse errors.
 */
public final class CompileInvalidSources {
    private CompileInvalidSources() {}

    /** Writes compiler error diagnostics for the provided source files to the target output file. */
    public static void main(String[] args) throws IOException {
        if (args.length < 3) {
            throw new IllegalArgumentException(
                    "Usage: CompileInvalidSources <output-dir> <diagnostics-file> <source-file>...");
        }

        Path outputDir = Path.of(args[0]);
        Path diagnosticsFile = Path.of(args[1]);
        List<Path> sourceFiles = new ArrayList<>();
        for (int i = 2; i < args.length; i++) {
            sourceFiles.add(Path.of(args[i]));
        }

        JavaCompiler compiler = ToolProvider.getSystemJavaCompiler();
        if (compiler == null) {
            throw new IllegalStateException("No system Java compiler available");
        }

        Files.createDirectories(outputDir);
        Files.createDirectories(diagnosticsFile.getParent());

        DiagnosticCollector<JavaFileObject> diagnostics = new DiagnosticCollector<>();

        try (StandardJavaFileManager fileManager =
                compiler.getStandardFileManager(diagnostics, Locale.ROOT, StandardCharsets.UTF_8)) {
            Iterable<? extends JavaFileObject> compilationUnits =
                    fileManager.getJavaFileObjectsFromPaths(sourceFiles);
            List<String> options =
                    List.of(
                            "-d",
                            outputDir.toString(),
                            "-Xmaxerrs",
                            "1000",
                            "-proc:none",
                            "-Xlint:none");
            JavacTask task =
                    (JavacTask)
                            compiler.getTask(
                                    null, fileManager, diagnostics, options, null, compilationUnits);
            Iterable<? extends CompilationUnitTree> trees = task.parse();
            for (CompilationUnitTree tree : trees) {
                if (tree.getSourceFile() == null) {
                    throw new IllegalStateException("Parsed tree has no source file");
                }
            }
            task.analyze();

            List<String> lines =
                    diagnostics.getDiagnostics().stream()
                            .filter(diagnostic -> diagnostic.getKind() == Diagnostic.Kind.ERROR)
                            .map(CompileInvalidSources::formatDiagnostic)
                            .collect(Collectors.toList());

            Files.write(diagnosticsFile, lines, StandardCharsets.UTF_8);

            if (lines.isEmpty()) {
                throw new IllegalStateException("javac succeeded but invalid sources should fail");
            }
        }
    }

    /** Formats diagnostics in the same shape that the Rust verifier expects from OpenJDK output. */
    private static String formatDiagnostic(Diagnostic<? extends JavaFileObject> diagnostic) {
        JavaFileObject source = diagnostic.getSource();
        String path = source == null ? "<unknown>" : Path.of(source.toUri()).toString();
        return String.format(
                Locale.ROOT,
                "%s:%d: error: %s",
                path,
                diagnostic.getLineNumber(),
                diagnostic.getMessage(Locale.ROOT));
    }
}
