final class C implements Traversable, IteratorAggregate {
    public function getIterator() {
        yield 1;
    }
}
                