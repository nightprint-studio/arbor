package corpus.legacy;

import java.awt.*;
import java.util.*;
import java.util.List;

/**
 * Two on-demand imports that both provide {@code List} ({@code java.awt.List}, {@code java.util.List}):
 * the single-type import wins, and the other one stays reachable fully qualified.
 */
public class ImportClash {

    public List<Point> corners(Rectangle box) {
        List<Point> out = new ArrayList<Point>();
        out.add(new Point(box.x, box.y));
        out.add(new Point(box.x + box.width, box.y + box.height));
        return out;
    }

    public Map<String, Color> palette() {
        Map<String, Color> colors = new LinkedHashMap<String, Color>();
        colors.put("red", Color.RED);
        colors.put("custom", new Color(10, 20, 30));
        return colors;
    }

    public int widget_rows(java.awt.List widget) {
        return widget.getRows();
    }

    public Dimension bounds_of(Collection<Point> points) {
        int width = 0;
        int height = 0;
        for (Point point : points) {
            width = Math.max(width, point.x);
            height = Math.max(height, point.y);
        }
        return new Dimension(width, height);
    }
}
