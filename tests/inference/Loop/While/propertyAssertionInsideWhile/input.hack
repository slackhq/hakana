final class Foo {
    public vec<mixed> $a = dict[];
    public vec<mixed> $b = dict[];
    public vec<mixed> $c = dict[];

    public function one(): bool {
        $has_changes = false;

        while (!C\is_empty($this->a)) {
            $has_changes = true;
            $this->alter();
        }

        return $has_changes;
    }

    public function two(): bool {
        $has_changes = false;

        while (!C\is_empty($this->a) || !C\is_empty($this->b)) {
            $has_changes = true;
            $this->alter();
        }

        return $has_changes;
    }

    public function three(): bool {
        $has_changes = false;

        while (!C\is_empty($this->a) || !C\is_empty($this->b) || !C\is_empty($this->c)) {
            $has_changes = true;
            $this->alter();
        }

        return $has_changes;
    }

    public function four(): bool {
        $has_changes = false;

        while ((!C\is_empty($this->a) && !C\is_empty($this->b)) || !C\is_empty($this->c)) {
            $has_changes = true;
            $this->alter();
        }

        return $has_changes;
    }

    public function alter() : void {
        if (rand(0, 1) !== 0) {
            $a = $this->a;
            array_pop(inout $a);
            $this->a = $a;
        } else if (rand(0, 1) !== 0) {
            $b = $this->b;
            array_pop(inout $b);
            $this->b = $b;
        } else {
            $c = $this->c;
            array_pop(inout $c);
            $this->c = $c;
        }
    }
}
