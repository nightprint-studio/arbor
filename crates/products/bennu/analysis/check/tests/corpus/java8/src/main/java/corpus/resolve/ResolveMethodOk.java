package corpus.resolve;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/** Legal twins of {@link ResolveMethodBad}, plus members that exist only implicitly. */
public class ResolveMethodOk {
    static class Helper {
        void help() {
        }

        static void staticHelp() {
        }
    }

    static class Base {
        protected void inheritedHelp() {
        }
    }

    static class Derived extends Base {
        void callsInheritedBare() {
            inheritedHelp();
        }

        void callsInheritedThroughSuper() {
            super.inheritedHelp();
        }
    }

    interface Named {
        String name();

        default String greeting() {
            return "hi " + name();
        }
    }

    enum Color {
        RED, GREEN
    }

    void helperLocal() {
    }

    void bareCallDeclared() {
        helperLocal();
    }

    void instanceCall(Helper helper) {
        helper.help();
    }

    void staticCall() {
        Helper.staticHelp();
    }

    void jdkStatic() {
        Math.max(1, 2);
    }

    void jdkInstance(String text) {
        text.length();
    }

    void knownAtTheEndOfAChain(List<String> names) {
        names.get(0).toUpperCase();
    }

    void callOnBoxed(Integer number) {
        number.toString();
    }

    void knownOnNewExpression() {
        new ArrayList<String>().add("x");
    }

    void objectMethodThroughSuper() {
        super.hashCode();
    }

    void declaredThroughThis() {
        this.helperLocal();
    }

    void apiInTheRelease(String text) {
        text.isEmpty();
    }

    void objectMethodsOnAnInterfaceType(Named named) {
        named.hashCode();
        named.getClass();
    }

    void defaultMethodOnAnInterfaceType(Named named) {
        named.greeting();
    }

    void enumSynthesizedMembers() {
        Color.valueOf("RED");
        Color.values();
        Color.RED.ordinal();
        Color.RED.name();
    }

    void arrayClone(int[] values) {
        int[] copy = values.clone();
    }

    void anonymousClassCallsOwnAndOuterMembers() {
        Runnable task = new Runnable() {
            public void run() {
                helperLocal();
                toString();
            }
        };
    }

    void localClassMethod() {
        class Local {
            int twice(int value) {
                return value * 2;
            }
        }
        new Local().twice(2);
    }

    void explicitTypeArgument() {
        Collections.<String>emptyList().size();
    }

    void builderChain() {
        new StringBuilder().append("a").append(1).reverse().toString();
    }
}
