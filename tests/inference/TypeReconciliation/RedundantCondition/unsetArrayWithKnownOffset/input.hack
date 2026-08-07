function bar(string $f) : void {
    $filter = rand(0, 1) !== 0 ? explode(",", $f) : vec[$f];
    unset($filter[rand(0, 1)]);
    if ($filter) {}
}
