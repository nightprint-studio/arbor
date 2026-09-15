package corpus.sealed;

import java.util.Objects;

/** A sealed interface whose permitted subtypes are records in the same file (no permits clause needed). */
public sealed interface Expr {

    record Num(double value) implements Expr {
    }

    record Add(Expr left, Expr right) implements Expr {
    }

    record Mul(Expr left, Expr right) implements Expr {
    }

    record Neg(Expr operand) implements Expr {
    }

    record Var(String name) implements Expr {
        public Var {
            Objects.requireNonNull(name, "name");
            if (name.isBlank()) {
                throw new IllegalArgumentException("blank variable name");
            }
        }
    }

    static Expr num(double value) {
        return new Num(value);
    }

    static Expr add(Expr left, Expr right) {
        return new Add(left, right);
    }

    static Expr times(Expr left, Expr right) {
        return new Mul(left, right);
    }

    static Expr variable(String name) {
        return new Var(name);
    }

    default Expr negate() {
        return new Neg(this);
    }
}
