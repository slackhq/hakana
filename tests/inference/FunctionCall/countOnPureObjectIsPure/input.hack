final class PureCountable implements \Countable {
    public function count()[]: int { return 1; }
}
function example(PureCountable $x)[] : int {
    return count($x);
}