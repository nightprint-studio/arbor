package corpus.sealed;

/**
 * An abstract sealed class whose permitted subclasses live in other files: a final one, a sealed one
 * with its own nested subclass, and a non-sealed one.
 */
public abstract sealed class Vehicle permits Car, Truck, Bicycle {

    private final String plate;

    protected Vehicle(String plate) {
        this.plate = plate;
    }

    public String plate() {
        return plate;
    }

    public abstract int wheels();

    /** Exhaustive without {@code default}: Car, Truck (covering Tanker) and Bicycle are all handled. */
    public static String toll(Vehicle vehicle) {
        return switch (vehicle) {
            case Car car when car.seats() > 5 -> "minibus " + car.plate();
            case Car car -> "car " + car.plate();
            case Truck.Tanker tanker -> "tanker " + tanker.plate() + " " + tanker.load();
            case Truck truck -> "truck " + truck.load();
            case Bicycle bicycle -> "free " + bicycle.plate() + " " + bicycle.wheels();
        };
    }

    public static int axles(Vehicle vehicle) {
        if (vehicle instanceof Truck truck && truck.load() > 10) {
            return 3;
        }
        return vehicle.wheels() / 2;
    }

    public static String demo() {
        return toll(new Car("AB123CD", 7)) + toll(new Truck.Tanker("TK1")) + toll(new Bicycle.Tandem())
                + axles(new Truck("TR9", 12.5));
    }
}
