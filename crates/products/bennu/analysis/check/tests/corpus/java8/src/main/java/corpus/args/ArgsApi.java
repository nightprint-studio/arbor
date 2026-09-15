package corpus.args;

import java.util.List;

/** Clean call targets shared by the argument cases. Must stay error-free: every case depends on it. */
public class ArgsApi {
    public ArgsApi() {
    }

    public ArgsApi(String name) {
    }

    public ArgsApi(String name, int size) {
    }

    public void none() {
    }

    public void one(int value) {
    }

    public void two(String name, int value) {
    }

    public String text(String value) {
        return value;
    }

    public static int twice(int value) {
        return value * 2;
    }

    public void varargs(String... values) {
    }

    public void leadingThenVarargs(int count, String... values) {
    }

    public void list(List<String> values) {
    }
}
