import java.lang.reflect.Constructor;
public class NP3 {
    static class Private { private Private() { System.out.println("Private<init>"); } }
    public static void main(String[] a) throws Exception {
        Constructor<?> k = Class.forName("NP3$Private").getDeclaredConstructor();
        k.newInstance();
    }
}
