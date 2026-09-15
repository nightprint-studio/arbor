package corpus.sealed;

/** A sealed permitted subclass whose own permitted subclass is nested in the same file. */
public sealed class Truck extends Vehicle {

    private final double load;

    public Truck(String plate, double load) {
        super(plate);
        this.load = load;
    }

    public double load() {
        return load;
    }

    @Override
    public int wheels() {
        return 6;
    }

    public static final class Tanker extends Truck {
        public Tanker(String plate) {
            super(plate, 30.0);
        }

        @Override
        public int wheels() {
            return 18;
        }
    }
}
