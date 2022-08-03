$gen = ((): Generator ==> {
    yield 1 => 'a';
    yield 2 => 'b';
})();

foreach ($gen as $k => $_) {
    echo "$k\n";
}