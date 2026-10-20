import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.nio.file.Paths;

/** Invokes only the embedded download fallback, without launching Minecraft or an EXE. */
public class BootstrapProbe {
    public static void main(String[] args) throws Exception {
        Class<?> bridge = Class.forName("org.jackhuang.hmcl.upgrade.UnBlockMcUpdate");
        Method ensure = bridge.getDeclaredMethod("ensureTool", Path.class, String.class);
        ensure.setAccessible(true);
        try {
            ensure.invoke(null, Paths.get(args[0]), args[1]);
        } catch (InvocationTargetException error) {
            throw new RuntimeException(error.getCause());
        }
    }
}
