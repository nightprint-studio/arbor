package corpus.structure;

import java.util.EnumMap;
import java.util.EnumSet;
import java.util.HashMap;
import java.util.Map;
import java.util.function.IntBinaryOperator;

/**
 * An enum with a constructor, constant bodies, an abstract method, an interface implemented per
 * constant, a static lookup table filled from {@code values()}, a nested enum, and a switch on itself.
 */
public enum Operation implements IntBinaryOperator {
    PLUS("+") {
        @Override
        public int applyAsInt(int left, int right) {
            return left + right;
        }

        @Override
        public String verb() {
            return "add";
        }
    },
    MINUS("-") {
        @Override
        public int applyAsInt(int left, int right) {
            return left - right;
        }

        @Override
        public String verb() {
            return "subtract";
        }
    },
    TIMES("*") {
        @Override
        public int applyAsInt(int left, int right) {
            return left * right;
        }

        @Override
        public String verb() {
            return "multiply";
        }

        @Override
        public boolean commutative() {
            return true;
        }
    };

    private static final Map<String, Operation> BY_SYMBOL = new HashMap<String, Operation>();

    static {
        for (Operation operation : values()) {
            BY_SYMBOL.put(operation.symbol, operation);
        }
    }

    private final String symbol;

    Operation(String symbol) {
        this.symbol = symbol;
    }

    public abstract String verb();

    public String symbol() {
        return symbol;
    }

    public boolean commutative() {
        return this == PLUS;
    }

    public enum Precedence {
        LOW(1), HIGH(2);

        private final int rank;

        Precedence(int rank) {
            this.rank = rank;
        }

        public boolean outranks(Precedence other) {
            return rank > other.rank;
        }
    }

    public Precedence precedence() {
        return this == TIMES ? Precedence.HIGH : Precedence.LOW;
    }

    public static Operation fromSymbol(String symbol) {
        Operation operation = BY_SYMBOL.get(symbol);
        if (operation == null) {
            throw new IllegalArgumentException("unknown operator " + symbol);
        }
        return operation;
    }

    /** Ends in a switch whose every group returns or throws: no trailing return is needed. */
    public static int evaluate(String expression) {
        String[] parts = expression.split(" ");
        int left = Integer.parseInt(parts[0]);
        int right = Integer.parseInt(parts[2]);
        Operation operation = fromSymbol(parts[1]);
        switch (operation) {
            case PLUS:
            case TIMES:
                return operation.applyAsInt(left, right);
            case MINUS:
                return left - right;
            default:
                throw new AssertionError(operation);
        }
    }

    public static Map<Operation, String> describeAll() {
        Map<Operation, String> out = new EnumMap<Operation, String>(Operation.class);
        for (Operation operation : EnumSet.range(PLUS, TIMES)) {
            out.put(operation, operation.name() + operation.ordinal() + operation.verb() + operation.symbol());
        }
        Operation minus = Operation.valueOf("MINUS");
        out.put(minus, "minus of " + values().length + " " + minus.commutative()
                + TIMES.precedence().outranks(minus.precedence()));
        return out;
    }
}
