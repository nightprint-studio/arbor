package corpus.sealed;

import java.util.HashMap;
import java.util.Map;

import corpus.sealed.Expr.Add;
import corpus.sealed.Expr.Mul;
import corpus.sealed.Expr.Neg;
import corpus.sealed.Expr.Num;
import corpus.sealed.Expr.Var;

/**
 * Exhaustive pattern switches over a sealed hierarchy without {@code default}, nested record patterns,
 * guards, record patterns in {@code instanceof}, and a binding that flows out of a negated {@code if}.
 */
public final class Evaluator {

    private final Map<String, Double> bindings;

    public Evaluator(Map<String, Double> bindings) {
        this.bindings = Map.copyOf(bindings);
    }

    public double eval(Expr expr) {
        return switch (expr) {
            case Num num -> num.value();
            case Add(Expr left, Expr right) -> eval(left) + eval(right);
            case Mul(Num(var factor), var right) when factor == 1.0 -> eval(right);
            case Mul(var left, var right) -> eval(left) * eval(right);
            case Neg(Neg(var inner)) -> eval(inner);
            case Neg(var operand) -> -eval(operand);
            case Var(String name) -> lookup(name);
        };
    }

    private double lookup(String name) {
        Double value = bindings.get(name);
        if (value == null) {
            throw new IllegalStateException("unbound variable " + name);
        }
        return value;
    }

    public Evaluator with(String name, double value) {
        var copy = new HashMap<>(bindings);
        copy.put(name, value);
        return new Evaluator(copy);
    }

    public static Expr simplify(Expr expr) {
        return switch (expr) {
            case Add(Num(var a), Num(var b)) -> new Num(a + b);
            case Mul(Num(var one), var other) when one == 1.0 -> simplify(other);
            case Add(var left, var right) -> new Add(simplify(left), simplify(right));
            case Mul(var left, var right) -> new Mul(simplify(left), simplify(right));
            case Neg(Num(var value)) -> new Num(-value);
            case Neg(var operand) -> new Neg(simplify(operand));
            case Num num -> num;
            case Var variable -> variable;
        };
    }

    public static String show(Expr expr) {
        if (expr instanceof Num(double value)) {
            return value == Math.rint(value) ? Long.toString((long) value) : Double.toString(value);
        }
        if (!(expr instanceof Add add)) {
            return switch (expr) {
                case Mul(var left, var right) -> "(" + show(left) + " * " + show(right) + ")";
                case Neg(var operand) -> "-" + show(operand);
                case Var(var name) -> name;
                case Num num -> Double.toString(num.value());
                case Add other -> show(other.left()) + "+" + show(other.right());
            };
        }
        return "(" + show(add.left()) + " + " + show(add.right()) + ")";
    }

    public static String demo() {
        var expr = Expr.add(Expr.num(2), Expr.times(Expr.num(1), Expr.variable("x"))).negate();
        var evaluator = new Evaluator(Map.of("x", 3.0)).with("y", 4);
        return show(simplify(expr)) + " = " + evaluator.eval(expr);
    }
}
