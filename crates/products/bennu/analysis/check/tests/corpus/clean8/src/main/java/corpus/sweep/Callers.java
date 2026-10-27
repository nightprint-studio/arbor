package corpus.sweep;

import corpus.sweep.lib.Failure;

import java.util.ArrayDeque;
import java.util.Queue;
import java.util.concurrent.FutureTask;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

public class Callers {
    Failure wrap() {
        return new Failure(null);
    }

    Failure many() {
        return new Failure(new String[0], null);
    }

    Object scheduled(ScheduledExecutorService executor) {
        FutureTask<Void> task = new FutureTask<Void>(new Runnable() {
            public void run() {
            }
        }, null);
        return executor.schedule(task, 1, TimeUnit.SECONDS);
    }

    int combine(Queue<byte[]> buffers) {
        int total = 0;
        for (byte[] chunk : buffers) {
            total += chunk.length;
        }
        return total;
    }

    int queued() {
        Queue<byte[]> queue = new ArrayDeque<byte[]>();
        queue.add(new byte[2]);
        return combine(queue);
    }
}
