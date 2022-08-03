final class C implements Traversable, Iterator {
    public function current() { return 1; }
    public function key() { return 1; }
    public function next() { }
    public function rewind() { }
    public function valid() { return false; }
}
                