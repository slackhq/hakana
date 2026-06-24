type mystring = string;

function foo(mystring ...$rest):void {}

$rest = vec["zzz"];

if (rand(0,1) !== 0) {
    $rest[] = "xxx";
}

foo("first", ...$rest);