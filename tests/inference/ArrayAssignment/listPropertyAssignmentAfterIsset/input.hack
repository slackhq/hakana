final class Collection {
    private vec<string> $list = dict[];

    public function override(int $offset): void {
        if (isset($this->list[$offset])) {
            $this->list[$offset] = "a";
        }
    }
}