package corpus.generics;

import java.util.ArrayList;
import java.util.List;

/** Self-typed ("curiously recurring") builders: every setter returns the concrete builder. */
public final class Builders {

    private Builders() {
    }

    public abstract static class Builder<B extends Builder<B>> {
        private String name = "";
        private int priority;

        protected abstract B self();

        public B name(String name) {
            this.name = name;
            return self();
        }

        public B priority(int priority) {
            this.priority = priority;
            return self();
        }

        protected String describeBase() {
            return name + "#" + priority;
        }
    }

    public static final class TaskBuilder extends Builder<TaskBuilder> {
        private final List<String> tags = new ArrayList<String>();

        @Override
        protected TaskBuilder self() {
            return this;
        }

        public TaskBuilder tag(String tag) {
            tags.add(tag);
            return this;
        }

        public String build() {
            return describeBase() + tags;
        }
    }

    /** Two type parameters, the second one self-typed, inherited through two levels. */
    public abstract static class ShapeBuilder<S, B extends ShapeBuilder<S, B>> extends Builder<B> {
        protected int width;

        public B width(int width) {
            this.width = width;
            return self();
        }

        public abstract S build();
    }

    public static final class Square {
        private final int side;
        private final String label;

        Square(int side, String label) {
            this.side = side;
            this.label = label;
        }

        public int side() {
            return side;
        }

        public String label() {
            return label;
        }
    }

    public static final class SquareBuilder extends ShapeBuilder<Square, SquareBuilder> {
        @Override
        protected SquareBuilder self() {
            return this;
        }

        public SquareBuilder doubled() {
            return width(width * 2);
        }

        @Override
        public Square build() {
            return new Square(width, describeBase());
        }
    }

    /** A generic static factory whose result is chained immediately. */
    public static <S, B extends ShapeBuilder<S, B>> S buildNamed(B builder, String name) {
        return builder.name(name).priority(1).build();
    }

    public static String demo() {
        String task = new TaskBuilder().name("deploy").priority(2).tag("ops").tag("night").build();
        Square square = new SquareBuilder().name("tile").width(4).doubled().priority(1).build();
        Square named = Builders.<Square, SquareBuilder>buildNamed(new SquareBuilder().width(3), "small");
        Square inferred = buildNamed(new SquareBuilder(), "empty");
        return task + square.side() + square.label() + named.label() + inferred.side();
    }
}
