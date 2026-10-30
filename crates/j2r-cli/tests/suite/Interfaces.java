interface Animal {
    int LEGS = 4;
    String sound();
}

interface Named {
    String NAME = "animal-system";
    String name();
}

class Dog implements Animal, Named {
    public String sound() { return "woof"; }
    public String name() { return "dog"; }
}

class Cat implements Animal, Named {
    public String sound() { return "meow"; }
    public String name() { return "cat"; }
}

public class Interfaces {
    static void speak(Animal a) {
        System.out.println(a.sound() + " with " + Animal.LEGS + " legs");
    }

    public static void main(String[] args) {
        Animal[] zoo = new Animal[2];
        zoo[0] = new Dog();
        zoo[1] = new Cat();
        for (Animal a : zoo) speak(a);
        Named n = new Dog();
        System.out.println(n.name());
        System.out.println(Named.NAME);
        System.out.println(zoo[0] instanceof Named);
        System.out.println(zoo[0] instanceof Animal);
        Object o = zoo[1];
        System.out.println(o instanceof Animal);
        Animal d = zoo[0];
        System.out.println(d.sound() + "!" + " from " + ((Named) d).name());
    }
}
