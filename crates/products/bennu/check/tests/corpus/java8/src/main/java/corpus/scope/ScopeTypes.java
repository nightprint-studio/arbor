package corpus.scope;

/** Clean members reached through inheritance, interfaces, static access and nesting. Must stay error-free. */
public final class ScopeTypes {
    private ScopeTypes() {
    }

    public static class Base {
        public void inherited(String value) {
        }

        protected static void staticInherited(int value) {
        }
    }

    public static class Sub extends Base {
    }

    public interface Greeter {
        String name();

        default String greet(String who) {
            return who + name();
        }

        static Greeter of(String name) {
            return () -> name;
        }
    }

    public static final class Util {
        private Util() {
        }

        public static int twice(int value) {
            return value * 2;
        }

        public static String join(String left, String right) {
            return left + right;
        }
    }

    public static class Outer {
        public static class Nested {
            public Nested(int size) {
            }

            public void work(String task) {
            }
        }

        public class Inner {
            public void work(int amount) {
            }
        }
    }

    public interface Limits {
        int MAX = 3;
    }

    public enum Level {
        LOW, HIGH
    }
}
