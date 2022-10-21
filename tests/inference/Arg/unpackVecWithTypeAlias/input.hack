type mystring = string;

function foo(mystring ...$rest):void {}

$rest = vec["zzz"];

if (rand(0,1)) {
    $rest[] = "xxx";
}

foo("first", ...$rest);