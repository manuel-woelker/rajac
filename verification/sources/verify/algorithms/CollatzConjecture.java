package verify.algorithms;

public class CollatzConjecture {
    public static int steps(int value) {
        int steps = 0;

        while (value > 1) {
            switch (value % 2) {
                case 0:
                    value = value / 2;
                    break;
                default:
                    value = value * 3 + 1;
            }
            steps = steps + 1;
        }

        return steps;
    }
}
