function mapper<Tv, To>(vec<Tv> $a, (function(Tv):To) $c): vec<To> {}

function takesIt(vec<string> $a): vec<int> {
    return mapper($a, ($a) ==> 5);
}

function takesItAgain(vec<string> $arr): vec<string> {
    return HH\Lib\Vec\sort($arr, ($a, $b) ==> $a <=> $b);
}