package corpus.sweep;

public interface DataIn {
    void readFully(byte b[]);

    void readFully(byte b[], int off, int len);

    class Impl implements DataIn {
        public void readFully(byte b[]) {
            readFully(b, 0, b.length);
        }

        public void readFully(byte b[], int off, int len) {
        }
    }
}
