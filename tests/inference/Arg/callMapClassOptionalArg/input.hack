final class Hello {}
$m = new ReflectionMethod(Hello::class, "goodbye");
$m->invoke(null, "cool");