final class C implements IteratorAggregate {
    <<__Override>>
    public function getIterator(): Iterator {
        return new ArrayIterator(vec[]);
    }
}