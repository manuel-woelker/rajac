package verify.controlflow;

public class BreakContinue {
    public static int accumulateOdds(int limit) {
        int sum = 0;
        int current = 0;

        while (true) {
            if (current > limit) {
                break;
            }
            if (current < limit) {
                current = current + 1;
                continue;
            }
            sum = sum + current;
            current = current + 1;
        }

        return sum;
    }
}
