function testarray(vec_or_dict $data): void {
    foreach ($data as $item) {
        if (isset($item["a"]) && isset($item["b"]["c"])) {
            echo "Found\n";
        }
    }
}