package corpus.flow;

import java.io.FileReader;
import java.io.IOException;

/** Checked exceptions neither caught nor declared. Twin: {@link FlowExceptionOk}. */
public class FlowExceptionBad {
    void throwsIoException() throws IOException {
    }

    static class FieldInitializerThrows {
        FileReader reader = new FileReader("file.txt"); // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void throwWithoutADeclaration() {
        throw new Exception("checked"); // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void projectCallWithoutADeclaration() {
        throwsIoException(); // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void jdkCallWithoutADeclaration() {
        Thread.sleep(1); // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void constructorWithoutADeclaration() {
        new FileReader("file.txt"); // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void unrelatedCatchDoesNotHandleIt() {
        try {
            throwsIoException(); // error: compiler.err.unreported.exception.need.to.catch.or.throw
        } catch (IllegalStateException ex) {
            System.gc();
        }
    }

    void rethrowOfTheCaughtException() {
        try {
            throwsIoException();
        } catch (IOException ex) {
            throw ex; // error: compiler.err.unreported.exception.need.to.catch.or.throw
        }
    }

    void insideARunnableLambda() {
        Runnable task = () -> throwsIoException(); // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void insideAnAnonymousRunnable() {
        Runnable task = new Runnable() { public void run() { throwsIoException(); } }; // error: compiler.err.unreported.exception.need.to.catch.or.throw
    }

    void methodReferenceThatThrows() {
        Runnable task = this::throwsIoException; // error: compiler.err.incompatible.thrown.types.in.mref
    }

    void implicitCloseThrows() {
        try (AutoCloseable resource = () -> { }) { // error: compiler.err.unreported.exception.implicit.close
            System.gc();
        }
    }
}
