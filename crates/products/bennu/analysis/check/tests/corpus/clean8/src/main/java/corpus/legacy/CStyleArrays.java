package corpus.legacy;

import java.util.ArrayList;
import java.util.List;

/** Legacy array declarators: the brackets follow the NAME (`String names[]`), and mean exactly what `String[] names` does. */
public class CStyleArrays {
    private static final int PRIMES[] = {2, 3, 5, 7};
    private String showletOutput[] = new String[3];
    private int grid[][] = new int[2][2];
    private List<String> lists[];

    int pick(int seed) {
        return PRIMES[Math.abs(seed) % PRIMES.length];
    }

    String first(String names[], int indexes[]) {
        int count = indexes.length;
        String head = names[count - 1];
        return head;
    }

    void locals() {
        int indexes[] = new int[showletOutput.length];
        for (int i = 0; i < indexes.length; i++) {
            indexes[i] = i;
        }
        String copy[] = showletOutput;
        String same = copy[0];
        int cell = grid[1][0];
        int row[] = grid[0];
        byte raw[] = new byte[4], second[] = new byte[2];
        long total = raw.length + second.length + row.length + cell;
        if (same == null && total > 0) {
            showletOutput[0] = "x";
        }
    }

    @SuppressWarnings("unchecked")
    List<String> mixed() {
        lists = new List[1];
        lists[0] = new ArrayList<String>();
        return lists[0];
    }

    String[] returned(String args[]) {
        return args;
    }

    /** A conditional of two different reference types, as a varargs element of a `format` call. */
    String message(String referer, StringBuffer url) {
        return String.format("Blocked %s to %s", referer, (referer != null ? referer : url));
    }

    String plain(String referer, StringBuffer url) {
        return String.format("Blocked %s to %s", referer, url);
    }

    String noParens(String referer, StringBuffer url) {
        return String.format("Blocked %s to %s", referer, referer != null ? referer : url);
    }

    String sameTypes(String referer, String other) {
        return String.format("Blocked %s to %s", referer, (referer != null ? referer : other));
    }

    String viaCalls(CStyleArrays self, String referer) {
        return String.format("Blocked %s to %s", self.name(), (self.name() != null ? self.name() : self.buffer()));
    }

    String name() {
        return "n";
    }

    StringBuffer buffer() {
        return new StringBuffer();
    }
}
