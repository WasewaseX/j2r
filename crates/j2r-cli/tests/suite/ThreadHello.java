public class ThreadHello {
    public static void main(String[] args) throws Exception {
        Thread t1 = new Thread(() -> System.out.println("worker 1"));
        Thread t2 = new Thread(() -> {
            System.out.println("worker 2a");
            Thread.yield();
            System.out.println("worker 2b");
        });
        Thread named = new Thread(() -> System.out.println("worker named"), "budget");
        t1.start();
        t1.join();
        t2.start();
        t2.join();
        named.start();
        named.join();
        // All output after the joins: this program's stdout is a pure
        // function of program order, on any scheduler (Temurin included).
        System.out.println("main done");
        Thread ct = Thread.currentThread();
        System.out.println("main name: " + ct.getName());
        System.out.println("active: " + Thread.activeCount());
    }
}
