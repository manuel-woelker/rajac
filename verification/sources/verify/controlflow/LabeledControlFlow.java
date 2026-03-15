package verify.controlflow;

public class LabeledControlFlow {
    public static int firstDiagonal(int limit) {
        int sum = 0;
        int zero = 0;

        outer: for (int row = 0; row < limit; row = row + 1) {
            int col = 0;
            while (col < limit) {
                if (row == col) {
                    sum = sum + row;
                    continue outer;
                }
                if (col > row) {
                    break outer;
                }
                col = col + 1;
            }
        }

        done: {
            sum = sum + 100;
            if (limit == zero) {
                break done;
            }
            sum = sum + 1;
        }

        return sum;
    }
}
