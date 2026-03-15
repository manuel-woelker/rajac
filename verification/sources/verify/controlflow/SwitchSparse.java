package verify.controlflow;

public class SwitchSparse {
    public static int map(int value) {
        switch (value) {
            case 1:
                return 10;
            case 10:
                return 20;
            case 100:
                return 30;
            default:
                return -1;
        }
    }
}
