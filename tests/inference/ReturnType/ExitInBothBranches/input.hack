function never_returns(int $a) : bool
{
    if ($a == 1) {
        throw new \Exception("one");
    } else {
        exit(0);
    }
}