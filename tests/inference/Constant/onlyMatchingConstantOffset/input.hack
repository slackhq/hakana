final class A {
    const KEYS = vec["one", "two", "three"];
    const ARR = dict[
        "one" => 1,
        "two" => 2
    ];
}

foreach (A::KEYS as $key) {
    if (isset(A::ARR[$key])) {
        echo A::ARR[$key];
    }
}