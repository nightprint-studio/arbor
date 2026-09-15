package corpus.mref;

/** Clean method-reference targets shared by the method-reference cases. Must stay error-free. */
public final class MrefTypes {
    private MrefTypes() {
    }

    public static class Named {
        public Named(String name) {
        }

        public int instanceLength(String text) {
            return text.length();
        }

        public static int staticLength(String text) {
            return text.length();
        }
    }
}
