package corpus.infer;

import corpus.infer.InferTypes.Animal;
import corpus.infer.InferTypes.Circle;
import corpus.infer.InferTypes.Color;
import corpus.infer.InferTypes.Dog;
import corpus.infer.InferTypes.Point;

import java.util.Arrays;

/** Members of the project's classes, enums and records, arrays, and inheritance. */
public class InferMembers {
    private final String label = "x";
    private int counter;
    static final long LIMIT = 10L;

    void enums() {
        Color[] values = Color.values(); // infer
        String name = Color.RED.name(); // infer
        int ordinal = Color.RED.ordinal(); // infer
        Color parsed = Color.valueOf("RED"); // infer
    }

    void records(Point point) {
        int x = point.x(); // infer
        Point origin = new Point(0, 0); // infer
        String text = point.toString(); // infer
        int hash = point.hashCode(); // infer
    }

    void arrays(String[] strings) {
        int[] numbers = new int[3]; // infer
        int element = numbers[0]; // infer
        int[][] grid = new int[2][2]; // infer
        int[] row = grid[0]; // infer
        int length = numbers.length; // infer
        String[] copy = Arrays.copyOf(strings, 2); // infer
        String first = strings[0]; // infer
        int[] literal = {1, 2};
    }

    void inheritance(Dog dog, Animal animal, Circle circle) {
        String speak = dog.speak(); // infer
        String kind = dog.kind(); // infer
        String fromBase = animal.speak(); // infer
        String described = circle.describe(); // infer
    }

    void own_members() {
        String fromField = label; // infer
        int fromCounter = counter; // infer
        long fromStatic = LIMIT; // infer
        String fromThis = this.label; // infer
        int increment = this.counter + 1; // infer
        long viaClass = InferMembers.LIMIT; // infer
    }

    int helper(int a) { return a; }

    String describe() { return label; }

    void own_methods() {
        int fromHelper = helper(1); // infer
        String fromDescribe = describe(); // infer
        String viaThis = this.describe(); // infer
        int nested = helper(helper(2)); // infer
    }
}
