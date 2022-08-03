function takesArguments(string $name, int $age) : void {}

$args = dict["name" =>  "hello", "age" => 5];
takesArguments(...$args);