package verify.controlflow;

public class SwitchDense {
    public static int map(int value) {
        switch (value) {
            case 0:
                return 10;
            case 1:
                return 20;
            case 2:
                return 30;
            default:
                return -1;
        }
    }
}
