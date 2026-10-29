final class Batch<T> {
    public function __construct(private AsyncIterator<vec<T>> $wrapped) {}

    public function removeNulls<Tn>(): Batch<Tn> where T = ?Tn {
        return new Batch(async (): AsyncIterator<vec<Tn>> ==> {
            foreach ($this->wrapped await as $batch) {
                yield \HH\Lib\Vec\filter_nulls($batch);
            }
        }());
    }
}
