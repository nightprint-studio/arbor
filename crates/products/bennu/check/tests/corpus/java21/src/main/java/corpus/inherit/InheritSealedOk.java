package corpus.inherit;

/** Legal twins of {@link InheritSealedBad}. */
public class InheritSealedOk {
    sealed interface Vehicle permits Car, Truck, Bike {
    }

    static final class Car implements Vehicle {
    }

    static non-sealed class Truck implements Vehicle {
    }

    static class PickupTruck extends Truck {
    }

    abstract static sealed class Bike implements Vehicle permits RoadBike {
    }

    static final class RoadBike extends Bike {
    }

    sealed interface PermitsInferredFromThisFile {
    }

    record Point(int x) implements PermitsInferredFromThisFile {
    }

    enum Direction implements PermitsInferredFromThisFile {
        NORTH
    }

    static final class Wrapper implements PermitsInferredFromThisFile {
    }
}
