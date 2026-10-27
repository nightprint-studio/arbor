package corpus.infer;

/** Literals, operators and conversions: the numeric promotions and the string concatenation rules. */
public class InferLiterals {
    void arithmetic(int a, long big, double real, float small, char ch, byte tiny, boolean flag, String name) {
        int sum = 1 + 2; // infer
        long product = 1L * 2; // infer
        double widened = 1 + 2.0; // infer
        float single = 1 + 2.0f; // infer
        long promoted = a + big; // infer
        double toDouble = a / real; // infer
        float toFloat = a * small; // infer
        int fromChar = ch + 1; // infer
        int fromByte = tiny + tiny; // infer
        int remainder = 7 % 3; // infer
        int shifted = 1 << 2; // infer
        long longShift = 1L << 2; // infer
        int negated = -a; // infer
        int bitwise = a & 3; // infer
        long bitwiseLong = big | a; // infer
        int increment = a++; // infer
    }

    void strings(int a, String name, char ch) {
        String joined = "a" + 1; // infer
        String withNull = name + null; // infer
        String withChar = ch + "x"; // infer
        String compound = 1 + 2 + "x"; // infer
        int length = "abc".length(); // infer
        char first = "abc".charAt(0); // infer
    }

    void booleans(int a, boolean flag, String name) {
        boolean less = 1 < 2; // infer
        boolean negated = !flag; // infer
        boolean both = flag && a > 1; // infer
        boolean equal = name == null; // infer
        boolean isString = name instanceof String; // infer
    }

    void conditionals(int a, long big, boolean flag) {
        int same = flag ? 1 : 2; // infer
        long mixed = flag ? 1 : 2L; // infer
        double wide = flag ? a : 2.5; // infer
        String maybe = flag ? "a" : null; // infer
    }

    void casts(int a, long big, double real, Object any) {
        byte narrowed = (byte) a; // infer
        long widened = (long) a; // infer
        int truncated = (int) real; // infer
        char asChar = (char) a; // infer
        Integer boxed = (Integer) any; // infer
        String text = (String) any; // infer
    }
}
