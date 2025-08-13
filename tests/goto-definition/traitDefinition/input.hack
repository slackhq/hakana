trait MyTrait {
    public function traitMethod(): string {
        return "trait";
    }
}

class ClassWithTrait {
    use MyTrait;
    
    public function useTraitMethod(): string {
        return $this->traitMethod();
    }
}