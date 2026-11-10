package corpus.equals;

/** Calls to {@code .equals()} where the two types are provably disjoint — the call can never
 *  return {@code true}. javac does not flag these (the parameter is typed {@code Object});
 *  Bennu reports {@code impossible-equals} as a warning. Twin: {@link ImpossibleEqualsOk}. */
public class ImpossibleEqualsBad {

    enum Color { RED, GREEN, BLUE }
    enum Size  { SMALL, MEDIUM, LARGE }

    void booleanReceiverStringArg(Boolean flag, String text) {
        flag.equals(text);
    }

    void stringReceiverBooleanArg(String text, Boolean flag) {
        text.equals(flag);
    }

    void integerReceiverStringArg(Integer number, String text) {
        number.equals(text);
    }

    void booleanLiteralReceiverStringArg(String text) {
        Boolean.TRUE.equals(text);
    }

    void differentEnumTypes(Color color, Size size) {
        color.equals(size);
    }

    void longReceiverIntPrimitiveArg(Long total, int count) {
        total.equals(count);
    }

    void integerReceiverLongPrimitiveArg(Integer count, long total) {
        count.equals(total);
    }

    void stringReceiverEnumArg(String text, Color color) {
        text.equals(color);
    }
}
