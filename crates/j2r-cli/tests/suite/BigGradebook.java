import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;

/**
 * BigGradebook — real-program-scale differential fixture on top of the new
 * JDK surface (Arrays.sort stability, Collections, subList views): a
 * course grade book that sorts rank lists, computes distributions, slices
 * honor rolls, and rebuilds rosters — the shape of ordinary business Java.
 */
public class BigGradebook {

    static final class Student {
        final String name;
        final int score;

        Student(String name, int score) {
            this.name = name;
            this.score = score;
        }

        public String toString() {
            return name + ":" + score;
        }
    }

    static final class Lcg {
        long state;

        Lcg(long seed) {
            this.state = seed;
        }

        int next(int bits) {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> (48 - bits));
        }
    }

    public static void main(String[] args) {
        Lcg rng = new Lcg(0x6B00C5);

        // Cohort of 40 students: names sNN, deterministic scores 40..100.
        Student[] cohort = new Student[40];
        for (int i = 0; i < cohort.length; i++) {
            cohort[i] = new Student("s" + (i < 10 ? "0" + i : i), 40 + rng.next(7) % 61);
        }

        // Report the raw roster (toString via the Object[] Arrays overload).
        System.out.println("roster: " + Arrays.toString(cohort));

        // Rank list: sort a COPY by score descending, tie -> name ascending.
        Student[] byScore = Arrays.copyOf(cohort, cohort.length);
        Arrays.sort(byScore, (a, b) -> {
            int byScoreDesc = b.score - a.score;
            return byScoreDesc != 0 ? byScoreDesc : a.name.compareTo(b.name);
        });
        System.out.println("top5: " + byScore[0] + " " + byScore[1] + " "
            + byScore[2] + " " + byScore[3] + " " + byScore[4]);

        // Score histogram over buckets [40-49..90-100] via [I fill/sort.
        int[] scores = new int[cohort.length];
        for (int i = 0; i < cohort.length; i++) {
            scores[i] = cohort[i].score;
        }
        int[] sorted = Arrays.copyOf(scores, scores.length);
        Arrays.sort(sorted);
        int[] buckets = new int[6];
        for (int i = 0; i < sorted.length; i++) {
            int s = sorted[i];
            int b = s < 100 ? (s - 40) / 10 : 5;
            if (b < 0) {
                b = 0;
            }
            buckets[b]++;
        }
        System.out.println("buckets=" + Arrays.toString(buckets));
        System.out.println("min=" + sorted[0] + " max=" + sorted[sorted.length - 1]
            + " median=" + (sorted[19] + sorted[20]) / 2);

        // Range slicing with copyOfRange: the top and bottom deciles.
        int n = sorted.length;
        System.out.println("bottom10=" + Arrays.toString(Arrays.copyOfRange(sorted, 0, 4)));
        System.out.println("top10=" + Arrays.toString(Arrays.copyOfRange(sorted, n - 4, n)));

        // Natural int[] sort must agree with the boxed path (equals check).
        Integer[] boxed = new Integer[n];
        for (int i = 0; i < n; i++) {
            boxed[i] = scores[i];
        }
        Arrays.sort(boxed);
        int[] reboxed = new int[n];
        for (int i = 0; i < n; i++) {
            reboxed[i] = boxed[i];
        }
        System.out.println("agree=" + Arrays.equals(sorted, reboxed));

        // Honor roll: a live subList view over the rank list's top-8.
        List<Student> rank = new ArrayList<Student>();
        Collections.addAll(rank, byScore);
        List<Student> honor = rank.subList(0, 8);
        StringBuilder hr = new StringBuilder();
        for (int i = 0; i < honor.size(); i++) {
            if (i > 0) {
                hr.append(",");
            }
            hr.append(honor.get(i).name);
        }
        System.out.println("honor roll: " + hr.toString());

        // The head of the cohort drops to 55 through the view (set writes
        // through to the parent).
        honor.set(0, new Student("s00", 55));
        System.out.println("after probation: rank[0]=" + rank.get(0)
            + " cohort[0]=" + cohort[0].name + "/" + cohort[0].score);

        // Probation list = the bottom 5 of the rank list, reversed.
        List<Student> tail = rank.subList(rank.size() - 5, rank.size());
        Collections.reverse(tail);
        StringBuilder prob = new StringBuilder();
        for (int i = 0; i < tail.size(); i++) {
            if (i > 0) {
                prob.append(",");
            }
            prob.append(tail.get(i).name);
        }
        System.out.println("probation: " + prob.toString());

        // Adding through the view grows the parent (insertion at the
        // subList boundary).
        int before = rank.size();
        tail.add(new Student("sX", 42));
        System.out.println("grew: " + (rank.size() == before + 1)
            + " last=" + rank.get(rank.size() - 1));

        // Structural parent mutation invalidates BOTH views.
        try {
            rank.remove(0);
            honor.get(0);
            System.out.println("unreachable");
        } catch (RuntimeException e) {
            System.out.println("cme: " + (e.getMessage() == null));
        }

        // Fresh views work; sort the whole rank list naturally by toString
        // identity is meaningless — sort by name with a comparator instead.
        List<Student> fresh = rank.subList(2, 6);
        Collections.sort(fresh, (a, b) -> a.name.compareTo(b.name));
        System.out.println("middle sorted: " + fresh);

        // Stability pin: re-sort the cohort copy by score only — equal
        // scores must keep the (name-ascending) order the previous sort
        // established.
        Arrays.sort(byScore, (a, b) -> b.score - a.score);
        StringBuilder stable = new StringBuilder();
        for (int i = 0; i < 10; i++) {
            if (i > 0) {
                stable.append(" ");
            }
            stable.append(byScore[i].name);
        }
        System.out.println("stable10: " + stable.toString());

        // fill: zero the histogram and refill from the (unchanged) scores.
        Arrays.fill(buckets, 0);
        System.out.println("zeroed=" + Arrays.toString(buckets));
        for (int i = 0; i < scores.length; i++) {
            int b = scores[i] < 100 ? (scores[i] - 40) / 10 : 5;
            if (b < 0) {
                b = 0;
            }
            buckets[b]++;
        }
        System.out.println("refilled=" + Arrays.toString(buckets));

        System.out.println("BigGradebook done");
    }
}
