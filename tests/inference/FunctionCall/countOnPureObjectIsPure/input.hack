final class PureCountable implements \Countable {
    <<__Override>>
    public function count()[]: int { return 1; }
}
function example(PureCountable $x)[] : int {
    return count($x);
}