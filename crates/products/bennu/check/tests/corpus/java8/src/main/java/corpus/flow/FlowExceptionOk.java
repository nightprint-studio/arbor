package corpus.flow;

import java.io.FileReader;
import java.io.IOException;
import java.io.StringWriter;
import java.util.concurrent.Callable;

/** Legal twins of {@link FlowExceptionBad}. */
public class FlowExceptionOk {
    void throwsIoException() throws IOException {
    }

    Object produce() throws IOException {
        return null;
    }

    static class ConstructorDeclaresTheInitializerException {
        FileReader reader = new FileReader("file.txt");

        ConstructorDeclaresTheInitializerException() throws IOException {
        }
    }

    void declaredThrow() throws Exception {
        throw new Exception("checked");
    }

    void declaredCall() throws IOException {
        throwsIoException();
    }

    void declaredAsASupertype() throws Exception {
        throwsIoException();
    }

    void caught() {
        try {
            throwsIoException();
        } catch (IOException ex) {
            System.gc();
        }
    }

    void caughtAsASupertype() {
        try {
            Thread.sleep(1);
        } catch (Exception ex) {
            System.gc();
        }
    }

    void preciseRethrow() throws IOException {
        try {
            throwsIoException();
        } catch (Exception ex) {
            throw ex;
        }
    }

    void uncheckedNeedsNoDeclaration() {
        throw new IllegalStateException();
    }

    void errorNeedsNoDeclaration() {
        throw new AssertionError();
    }

    void callableLambdaMayThrow() {
        Callable<Void> task = () -> {
            throwsIoException();
            return null;
        };
    }

    void callableMethodReferenceMayThrow() {
        Callable<Object> task = this::produce;
    }

    void anonymousCallableMayThrow() {
        Callable<Void> task = new Callable<Void>() {
            public Void call() throws IOException {
                throwsIoException();
                return null;
            }
        };
    }

    void implicitCloseDeclared() throws Exception {
        try (AutoCloseable resource = () -> { }) {
            System.gc();
        }
    }

    void implicitCloseCaught() {
        try (StringWriter writer = new StringWriter()) {
            writer.write("x");
        } catch (IOException ex) {
            System.gc();
        }
    }
}
