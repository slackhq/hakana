final class A {}

abstract class B {}

interface C {}

<<__Sealed()>>
class D {}

class E extends \Exception {}

class F {}

/* HHAST_FIXME[FinalOrAbstractClass] */
class G {}
