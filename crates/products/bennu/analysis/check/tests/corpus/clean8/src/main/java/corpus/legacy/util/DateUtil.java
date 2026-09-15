package corpus.legacy.util;

import java.util.Calendar;
import java.util.Date;

/** Constants and helpers imported on demand with {@code import static corpus.legacy.util.DateUtil.*}. */
public final class DateUtil {

    public static final long DAY_MILLIS = 24L * 60 * 60 * 1000;
    public static final int RECENT_DAYS = 30;

    private DateUtil() {
    }

    public static boolean is_recent(Date date) {
        return date != null && System.currentTimeMillis() - date.getTime() < RECENT_DAYS * DAY_MILLIS;
    }

    public static Date start_of_day(Date date) {
        Calendar calendar = Calendar.getInstance();
        calendar.setTime(date);
        calendar.set(Calendar.HOUR_OF_DAY, 0);
        calendar.set(Calendar.MINUTE, 0);
        calendar.set(Calendar.SECOND, 0);
        calendar.set(Calendar.MILLISECOND, 0);
        return calendar.getTime();
    }
}
