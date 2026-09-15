package corpus.inherit;

import java.io.IOException;
import java.util.List;

/** Legal twins of {@link InheritOverrideBad}. */
public class InheritOverrideOk {
    static class RealOverride extends InheritTypes.OpenBase {
        @Override
        public void open() {
        }
    }

    static class CovariantReturnType extends InheritTypes.OpenBase {
        @Override
        public String produce() {
            return "";
        }
    }

    static class Risky {
        void risky() throws Exception {
        }
    }

    static class ThrowsNothing extends Risky {
        @Override
        void risky() {
        }
    }

    static class ThrowsASubtype extends Risky {
        @Override
        void risky() throws IOException {
        }
    }

    static class StaticHidesStatic extends InheritTypes.OpenBase {
        public static void helper() {
        }
    }

    static class ObjectMethods {
        @Override
        public boolean equals(Object other) {
            return this == other;
        }

        @Override
        public int hashCode() {
            return 1;
        }

        @Override
        public String toString() {
            return "";
        }
    }

    interface WithDefault {
        default String describe() {
            return "";
        }
    }

    static class OverridesADefaultMethod implements WithDefault {
        @Override
        public String describe() {
            return "x";
        }
    }

    interface Source<T> {
        T next(List<? extends T> pool);
    }

    static class GenericOverride implements Source<String> {
        @Override
        public String next(List<? extends String> pool) {
            return pool.get(0);
        }
    }

    static class OverridesAbstractMethods extends InheritTypes.Shape {
        @Override
        protected double area() {
            return 0;
        }

        @Override
        public String label() {
            return "";
        }
    }
}
