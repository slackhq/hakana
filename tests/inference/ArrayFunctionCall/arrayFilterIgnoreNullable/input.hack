final class A {
    public function getRows() : vec<?A> {
        return vec[new A(), null];
    }

    public function filter() : void {
        $arr = array_filter(
            static::getRows(),
            (A $row): bool ==> {
                return is_a($row, static::class);
            }
        );
    }
}