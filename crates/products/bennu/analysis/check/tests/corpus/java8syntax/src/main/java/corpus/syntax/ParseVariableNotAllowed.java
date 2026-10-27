package corpus.syntax;

public class ParseVariableNotAllowed {
    void run(boolean flag) {
        if (flag) int local = 1; // error: compiler.err.variable.not.allowed
    }
}
