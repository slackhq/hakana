final class C implements IteratorAggregate {
    public function getIterator(): Iterator {
        return new ArrayIterator(vec[]);
    }
}