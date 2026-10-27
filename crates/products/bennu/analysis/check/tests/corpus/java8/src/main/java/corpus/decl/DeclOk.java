package corpus.decl;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

/** The legal shapes of what {@link DeclBad} gets wrong. */
public class DeclOk {
    static abstract class Shape { }

    static class Square extends Shape { }

    enum Color {
        RED, GREEN;
        static final int COUNT = 2;
        private static int mutable;

        Color() { }

        static void bump() { mutable++; }
    }

    Shape shape = new Square();

    Shape anonymous = new Shape() { };

    Color color = Color.RED;

    static class Once implements Runnable, Comparable<Once> {
        public void run() { }
        public int compareTo(Once other) { return 0; }
    }

    class Inner {
        static final int CONSTANT = 1;
        void instanceMethod() { }
    }

    static class StaticHolder {
        static void staticMethod() { }
        interface Nested { }
    }

    List<String> diamond = new ArrayList<>();

    List<String> diamondFromCollection = new ArrayList<>(diamond);

    <T> T identity(T value) { return value; }

    String explicit() {
        return this.<String>identity("x");
    }

    Comparator<String> viaType() {
        return Comparator.naturalOrder();
    }

    static int first = 1;
    static int second = first + 1;
}

class Visible {
    int count;
    void touch() { }
}

class Neighbour {
    int peek(Visible visible) {
        visible.touch();
        return visible.count;
    }
}
