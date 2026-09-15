package corpus.args;

import java.util.Arrays;

/** Legal twins of {@link ArgsCountBad}, plus arities only varargs make legal. */
public class ArgsCountOk {
    void local(int value) {
    }

    void exactOne(ArgsApi api) {
        api.one(1);
    }

    void exactTwo(ArgsApi api) {
        api.two("a", 1);
    }

    void noArgs(ArgsApi api) {
        api.none();
    }

    void staticExact() {
        ArgsApi.twice(1);
    }

    void bareExact() {
        local(1);
    }

    void varargsLeadingOnly(ArgsApi api) {
        api.leadingThenVarargs(1);
    }

    void jdkExact(String text) {
        text.isEmpty();
    }

    void jdkOverloads(String text) {
        text.substring(1);
        text.substring(1, 2);
    }

    void throughThisExact() {
        this.local(1);
    }

    void varargsEmpty(ArgsApi api) {
        api.varargs();
    }

    void varargsMany(ArgsApi api) {
        api.varargs("a", "b", "c");
    }

    void varargsGivenAnArray(ArgsApi api) {
        api.varargs(new String[] { "a" });
    }

    void varargsNullElement(ArgsApi api) {
        api.varargs((String) null);
    }

    void varargsNullArray(ArgsApi api) {
        api.varargs((String[]) null);
    }

    void jdkVarargs() {
        String.format("%s-%s", "a", 1);
        Arrays.asList(1, 2, 3);
    }

    void callsAsArguments(ArgsApi api) {
        api.one(ArgsApi.twice(api.text("x").length()));
    }
}
