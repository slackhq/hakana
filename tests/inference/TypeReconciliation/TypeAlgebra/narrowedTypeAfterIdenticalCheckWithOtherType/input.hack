function a(int $a, ?int $b = null): void
{
    if ($a === $b) {
        throw new InvalidArgumentException(sprintf("a can not be the same as b (b: %s).", $b));
    }
}