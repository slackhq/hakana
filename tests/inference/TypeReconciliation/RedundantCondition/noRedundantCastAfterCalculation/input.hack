function x(string $x): int {
    return (int) (hexdec($x) + 1);
}