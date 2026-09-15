package corpus.sealed;

/** A non-sealed permitted subclass: anything may extend it again. */
public non-sealed class Bicycle extends Vehicle {

    public Bicycle(String plate) {
        super(plate);
    }

    @Override
    public int wheels() {
        return 2;
    }

    public static class Tandem extends Bicycle {
        public Tandem() {
            super("tandem");
        }
    }
}
