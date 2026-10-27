package corpus.stmt;

import java.io.IOException;
import java.io.StringReader;

/** The legal shapes of what {@link StmtBad} gets wrong: reachable code, jumps with a target, catches that can fire. */
public class StmtOk {
    int reachableAfterBranch(boolean flag) {
        if (flag) {
            return 1;
        }
        int live = 2;
        return live;
    }

    void jumpsWithTargets() {
        outer:
        for (int i = 0; i < 3; i++) {
            for (int j = 0; j < 3; j++) {
                if (j == 1) continue outer;
                if (i == 2) break outer;
            }
        }
        block: {
            if (System.nanoTime() > 0) break block;
        }
    }

    void endlessLoopWithExit() {
        while (true) {
            if (System.nanoTime() > 0) break;
        }
        int live = 1;
    }

    void catchesThatCanFire() throws IOException {
        try {
            throw new IOException();
        } catch (java.io.FileNotFoundException e) {
        } catch (IOException e) {
        }
        try {
            int calm = 1;
        } catch (RuntimeException e) {
        } catch (Exception e) {
        }
    }

    void disjointMulticatch() {
        try {
            throw new IOException();
        } catch (IOException | IllegalStateException e) {
        }
    }

    void readResource() throws IOException {
        try (StringReader reader = new StringReader("")) {
            reader.read();
        }
    }

    boolean comparable(Integer a, Number b, Object c) {
        return a == b || c == a || a == 1;
    }

    int[] arrays() {
        int[] numbers = {1, 2};
        numbers[0] = numbers[1];
        return numbers;
    }

    int notSelfReferential = 1;
    int next = notSelfReferential + 1;
}
