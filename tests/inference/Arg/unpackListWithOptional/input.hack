function foo(string ...$rest):void {}

$rest = vec["zzz"];

if (rand(0,1)) {
    $rest[] = "xxx";
}

foo("first", ...$rest);