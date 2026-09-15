package corpus.resolve;

import java.util.ArrayList;
import java.util.List;

/** Calls to methods that do not exist. Twin: {@link ResolveMethodOk}. */
public class ResolveMethodBad {
    static class Helper {
        void help() {
        }

        static void staticHelp() {
        }
    }

    void helperLocal() {
    }

    void bareCallUnknown() {
        undefinedMethod(); // error: compiler.err.cant.resolve.location.args
    }

    void bareCallMisspelled() {
        helperLocl(); // error: compiler.err.cant.resolve.location.args
    }

    void instanceCallUnknown(Helper helper) {
        helper.helpMe(); // error: compiler.err.cant.resolve.location.args
    }

    void staticCallUnknown() {
        Helper.staticHelpMe(); // error: compiler.err.cant.resolve.location.args
    }

    void jdkStaticUnknown() {
        Math.maximum(1, 2); // error: compiler.err.cant.resolve.location.args
    }

    void jdkInstanceMisspelled(String text) {
        text.lenght(); // error: compiler.err.cant.resolve.location.args
    }

    void unknownAtTheEndOfAChain(List<String> names) {
        names.get(0).toUpperCas(); // error: compiler.err.cant.resolve.location.args
    }

    void callOnPrimitive(int number) {
        number.toString(); // error: compiler.err.cant.deref
    }

    void unknownReceiverVariable() {
        missingReceiver.call(); // error: compiler.err.cant.resolve.location
    }

    void unknownOnNewExpression() {
        new ArrayList<String>().push("x"); // error: compiler.err.cant.resolve.location.args
    }

    void unknownThroughSuper() {
        super.notInObject(); // error: compiler.err.cant.resolve.args
    }

    void misspelledThroughThis() {
        this.helperLocl(); // error: compiler.err.cant.resolve.args
    }

    void apiNewerThanTheRelease(String text) {
        text.isBlank(); // error: compiler.err.cant.resolve.location.args
    }
}
