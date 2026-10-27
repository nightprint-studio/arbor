package corpus.stmt;

import java.io.IOException;
import java.io.StringReader;

/** Statements that are legal syntax but wrong in context: unreachable code, stray jumps, impossible catches. Twin: {@link StmtOk}. */
public class StmtBad {
    void afterReturn() {
        return;
        int dead = 1; // error: compiler.err.unreachable.stmt
    }

    void afterBreak() {
        for (;;) {
            break;
            int dead = 1; // error: compiler.err.unreachable.stmt
        }
    }

    void afterEndlessLoop() {
        while (true) { }
        int dead = 1; // error: compiler.err.unreachable.stmt
    }

    void strayBreak() {
        break; // error: compiler.err.break.outside.switch.loop
    }

    void strayContinue() {
        continue; // error: compiler.err.cont.outside.loop
    }

    void unknownLabel() {
        for (;;) {
            break missing; // error: compiler.err.undef.label
        }
    }

    void notALoopLabel() {
        block: {
            for (;;) {
                continue block; // error: compiler.err.not.loop.label
            }
        }
    }

    void neverThrown() {
        try {
            int calm = 1;
        } catch (IOException e) { // error: compiler.err.except.never.thrown.in.try
        }
    }

    void alreadyCaught() {
        try {
            throw new IOException();
        } catch (Exception e) {
        } catch (IOException e) { // error: compiler.err.except.already.caught
        }
    }

    void overlappingMulticatch() {
        try {
            throw new IOException();
        } catch (IOException | Exception e) { // error: compiler.err.multicatch.types.must.be.disjoint
        }
    }

    void assignedResource() throws IOException {
        try (StringReader reader = new StringReader("")) {
            reader = null; // error: compiler.err.try.resource.may.not.be.assigned
        }
    }

    boolean incomparable() {
        return Integer.valueOf(1) == "one"; // error: compiler.err.incomparable.types
    }

    void nothing() { }

    String voidOperand() {
        return "a" + nothing(); // error: compiler.err.void.not.allowed.here
    }

    int indexNotArray() {
        int number = 1;
        return number[0]; // error: compiler.err.array.req.but.found
    }

    int selfRef = selfRef + 1; // error: compiler.err.illegal.self.ref
}
