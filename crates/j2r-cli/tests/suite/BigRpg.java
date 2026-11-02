import java.util.ArrayList;
import java.util.HashMap;

/**
 * BigRpg — real-program-scale differential fixture: a small battle
 * simulation with an entity hierarchy, inventories, a hand-rolled merge
 * sort, lambda-driven party management, and a deterministic seeded LCG
 * (the runtime has no java.util.Random yet, and an unseeded one would not
 * be diffable anyway).
 *
 * Everything prints a transcript that must match Temurin byte-for-byte.
 */
public class BigRpg {

    // ------------------------------------------------------------------
    // Deterministic LCG (java.util.Random is out of the supported surface)
    // ------------------------------------------------------------------

    static final class Lcg {
        long state;

        Lcg(long seed) {
            this.state = seed;
        }

        int next(int bits) {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> (48 - bits));
        }

        int below(int bound) {
            // Rejection-free approximate bound sampling: good enough for a
            // sim, perfectly deterministic.
            if (bound <= 0) {
                throw new IllegalArgumentException("bound must be positive");
            }
            return java.lang.Math.abs(next(20)) % bound;
        }
    }

    // ------------------------------------------------------------------
    // Entities
    // ------------------------------------------------------------------

    interface Damageable {
        void takeDamage(int amount, String source);
        boolean isAlive();
    }

    interface Lootable {
        String loot();
    }

    abstract static class Entity implements Damageable {
        final String name;
        int hp;
        final int maxHp;
        final ArrayList<String> log = new ArrayList<String>();

        Entity(String name, int hp) {
            this.name = name;
            this.hp = hp;
            this.maxHp = hp;
        }

        public boolean isAlive() {
            return hp > 0;
        }

        public void takeDamage(int amount, String source) {
            if (amount < 0) {
                throw new IllegalArgumentException("negative damage from " + source);
            }
            hp -= amount;
            if (hp < 0) {
                hp = 0;
            }
            log.add(name + " takes " + amount + " from " + source + " -> " + hp);
        }

        void heal(int amount) {
            hp = java.lang.Math.min(maxHp, hp + amount);
            log.add(name + " heals " + amount + " -> " + hp);
        }

        abstract int attack(Lcg rng);

        public String toString() {
            return name + "(" + hp + "/" + maxHp + ")";
        }
    }

    static final class Player extends Entity {
        int level;
        final ArrayList<String> inventory = new ArrayList<String>();

        Player(String name, int hp, int level) {
            super(name, hp);
            this.level = level;
        }

        int attack(Lcg rng) {
            int base = 4 + level;
            return base + rng.below(4);
        }

        void pickUp(String item) {
            inventory.add(item);
        }

        public String loot() {
            if (inventory.isEmpty()) {
                return "nothing";
            }
            return inventory.remove(inventory.size() - 1);
        }
    }

    static class Monster extends Entity implements Lootable {
        final int power;

        Monster(String name, int hp, int power) {
            super(name, hp);
            this.power = power;
        }

        int attack(Lcg rng) {
            return power + rng.below(3);
        }

        public void takeDamage(int amount, String source) {
            // Monsters flinch: every third hit is halved (floor division).
            if (hitCount % 3 == 2 && amount > 1) {
                amount = amount / 2;
            }
            hitCount++;
            super.takeDamage(amount, source);
        }

        int hitCount;

        public String loot() {
            return name + "'s tooth";
        }
    }

    static final class Dragon extends Monster {
        int breathsLeft = 2;

        Dragon(String name, int hp, int power) {
            super(name, hp, power);
        }

        int attack(Lcg rng) {
            if (breathsLeft > 0 && rng.below(3) == 0) {
                breathsLeft--;
                return power * 3;
            }
            return super.attack(rng);
        }
    }

    // ------------------------------------------------------------------
    // Merge sort (the suite has no Collections.sort on this surface)
    // ------------------------------------------------------------------

    static void mergeSort(ArrayList<Player> list, java.util.Comparator<Player> cmp) {
        if (list.size() < 2) {
            return;
        }
        int mid = list.size() / 2;
        // Explicit copies (ArrayList.subList view semantics are not on the
        // supported surface yet — documented, not silently downgraded).
        ArrayList<Player> left = new ArrayList<Player>();
        ArrayList<Player> right = new ArrayList<Player>();
        for (int t = 0; t < mid; t++) {
            left.add(list.get(t));
        }
        for (int t = mid; t < list.size(); t++) {
            right.add(list.get(t));
        }
        mergeSort(left, cmp);
        mergeSort(right, cmp);
        int i = 0;
        int j = 0;
        int k = 0;
        while (i < left.size() && j < right.size()) {
            if (cmp.compare(left.get(i), right.get(j)) <= 0) {
                list.set(k++, left.get(i++));
            } else {
                list.set(k++, right.get(j++));
            }
        }
        while (i < left.size()) {
            list.set(k++, left.get(i++));
        }
        while (j < right.size()) {
            list.set(k++, right.get(j++));
        }
    }

    // ------------------------------------------------------------------
    // Simulation
    // ------------------------------------------------------------------

    static String battle(Player p, Monster m, Lcg rng) {
        int round = 1;
        while (p.isAlive() && m.isAlive()) {
            int pd = p.attack(rng);
            m.takeDamage(pd, p.name);
            if (!m.isAlive()) {
                break;
            }
            int md = m.attack(rng);
            p.takeDamage(md, m.name);
            round++;
        }
        String winner = p.isAlive() ? p.name : m.name;
        return "battle: " + p + " vs " + m + " -> " + winner + " in " + round + " rounds";
    }

    public static void main(String[] args) {
        Lcg rng = new Lcg(0x5EED_1234_ABCD_0001L);

        Player hero = new Player("Aster", 30, 3);
        Player mage = new Player("Brix", 22, 4);
        Player rogue = new Player("Cyra", 26, 2);
        ArrayList<Player> party = new ArrayList<Player>();
        party.add(hero);
        party.add(mage);
        party.add(rogue);

        // Lambda-driven party prep.
        party.forEach(pl -> pl.pickUp("potion"));
        party.forEach(pl -> pl.pickUp("rope"));
        party.removeIf(pl -> pl.name.equals("Brix") ? false : false);
        System.out.println("party size " + party.size());

        // Hand-rolled sort by (hp asc, name asc) via comparator lambda.
        mergeSort(party, (a, b) -> {
            int byHp = Integer.compare(a.hp, b.hp);
            return byHp != 0 ? byHp : a.name.compareTo(b.name);
        });
        StringBuilder order = new StringBuilder();
        for (int i = 0; i < party.size(); i++) {
            if (i > 0) {
                order.append(", ");
            }
            order.append(party.get(i).name);
        }
        System.out.println("sorted: " + order.toString());

        // Three battles: goblin, orc, dragon (subclass override chain).
        System.out.println(battle(party.get(0), new Monster("Grub", 12, 3), rng));
        System.out.println(battle(party.get(1), new Monster("Snarl", 18, 5), rng));
        Dragon d = new Dragon("Emberwing", 40, 6);
        System.out.println(battle(party.get(2), d, rng));
        System.out.println("dragon breaths left " + d.breathsLeft);

        // Loot via the interface.
        ArrayList<Lootable> spoils = new ArrayList<Lootable>();
        spoils.add(new Monster("Fallen Grub", 0, 1));
        spoils.add(d);
        for (int i = 0; i < spoils.size(); i++) {
            System.out.println("loot " + i + ": " + spoils.get(i).loot());
        }

        // Post-battle state and healing.
        party.forEach(pl -> pl.heal(5));
        party.forEach(pl -> System.out.println(pl.toString() + " log=" + pl.log.size()));

        // Stats aggregated in a map (iteration order is pinned by the
        // HashMapOrder fixture, so this is diffable).
        HashMap<String, Integer> stats = new HashMap<String, Integer>();
        stats.put("battles", 3);
        stats.put("rounds", 12);
        stats.put("potions", 2);
        int total = 0;
        for (Integer v : stats.values()) {
            total += v;
        }
        System.out.println("stat total " + total);
        System.out.println("has rounds " + stats.containsKey("rounds"));
        System.out.println("missing " + stats.getOrDefault("wins", -1));

        // Exception paths inside the sim: negative damage is a bug.
        try {
            hero.takeDamage(-1, "glitch");
            System.out.println("unreachable");
        } catch (IllegalArgumentException e) {
            System.out.println("caught: " + e.getMessage());
        }

        // The logs: one line per damage/heal event, in order.
        System.out.println("hero log:");
        for (int i = 0; i < hero.log.size(); i++) {
            System.out.println("  " + i + ": " + hero.log.get(i));
        }

        System.out.println("BigRpg done");
    }
}
