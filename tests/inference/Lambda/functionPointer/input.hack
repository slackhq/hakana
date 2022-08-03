function foo(string $a): string {
    return $a . "blah";
}

$bar = vec["foo", "bar"];

$bam = \HH\Lib\Vec\map(
    $bar,
    foo<>
);

hakana_expect_type<vec<string>>($bam);