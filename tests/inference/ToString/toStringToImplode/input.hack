class Bar {
    public function __toString() {
        return "foo";
    }
}

echo implode(":", vec[new Bar()]);