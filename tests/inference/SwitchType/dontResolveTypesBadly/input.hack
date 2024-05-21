$a = new A();

switch (rand(0,1)) {
    case 0:
    case 1:
        $dt = $a->maybeReturnsDT();
        if (!$dt is null) {
            $dt = $dt->format(\DateTime::ISO8601);
        }
        break;
}

final class A {
    public function maybeReturnsDT(): ?\DateTimeInterface {
        return rand(0,1) ? new \DateTime("now") : null;
    }
}