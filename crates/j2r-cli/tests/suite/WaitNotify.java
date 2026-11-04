public class WaitNotify {
    static Object mailbox = new Object();
    static String msg = "";
    static boolean ready = false;

    static Object board = new Object();
    static int phase = 0;
    static int boardCount = 0;

    static Object deepLock = new Object();
    static boolean deepGo = false;
    static int deepCount = 0;

    public static void main(String[] args) throws Exception {
        // 1) producer/consumer handoff. Correctness never depends on the
        // sleep: the flag is latched before the notify, and the consumer
        // rechecks it, so either scheduling produces the same output.
        Thread consumer = new Thread(() -> {
            synchronized (mailbox) {
                while (!ready) {
                    try {
                        mailbox.wait();
                    } catch (InterruptedException e) {
                        System.out.println("c interrupted");
                    }
                }
                System.out.println("got " + msg);
            }
        });
        Thread producer = new Thread(() -> {
            synchronized (mailbox) {
                msg = "ping-42";
                ready = true;
                mailbox.notify();
            }
        });
        consumer.start();
        Thread.sleep(50);
        producer.start();
        consumer.join();
        producer.join();

        // 2) notifyAll: three waiters must all wake; each re-acquires the
        // monitor to bump the counter, so the total is exact.
        Thread w1 = new Thread(() -> {
            synchronized (board) {
                while (phase < 1) {
                    try {
                        board.wait();
                    } catch (InterruptedException e) {
                    }
                }
                boardCount += 1;
            }
        });
        Thread w2 = new Thread(() -> {
            synchronized (board) {
                while (phase < 1) {
                    try {
                        board.wait();
                    } catch (InterruptedException e) {
                    }
                }
                boardCount += 1;
            }
        });
        Thread w3 = new Thread(() -> {
            synchronized (board) {
                while (phase < 1) {
                    try {
                        board.wait();
                    } catch (InterruptedException e) {
                    }
                }
                boardCount += 1;
            }
        });
        w1.start();
        w2.start();
        w3.start();
        Thread.sleep(50);
        synchronized (board) {
            phase = 1;
            board.notifyAll();
        }
        w1.join();
        w2.join();
        w3.join();
        System.out.println("board " + boardCount);

        // 3) IllegalMonitorStateException without ownership: wait, notify,
        // notifyAll all require the current thread to own the monitor.
        try {
            mailbox.wait();
        } catch (IllegalMonitorStateException e) {
            System.out.println("imse-wait");
        }
        try {
            mailbox.notify();
        } catch (IllegalMonitorStateException e) {
            System.out.println("imse-notify");
        }
        try {
            mailbox.notifyAll();
        } catch (IllegalMonitorStateException e) {
            System.out.println("imse-notifyall");
        }

        // 4) wait() releases only one hold: a depth-2 re-entrant owner waits
        // and, on wake, must come back with the FULL saved count (both holds
        // restored) or the inner/outer exits would throw. Re-entering once
        // more before leaving proves ownership survived the round trip.
        Thread deep = new Thread(() -> {
            synchronized (deepLock) {
                synchronized (deepLock) {
                    while (!deepGo) {
                        try {
                            deepLock.wait();
                        } catch (InterruptedException e) {
                        }
                    }
                    deepCount += 1;
                    synchronized (deepLock) {
                        deepCount += 10;
                    }
                }
            }
        });
        deep.start();
        Thread.sleep(50);
        synchronized (deepLock) {
            deepGo = true;
            deepLock.notify();
        }
        deep.join();
        System.out.println("deep " + deepCount);

        System.out.println("done");
    }
}
