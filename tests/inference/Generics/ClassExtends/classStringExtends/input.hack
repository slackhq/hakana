abstract class Maker {}

final class GlassMaker extends Maker {
    public function getEventClass(): classname<MakerEvent<this>> {
        return GlassMakerEvent::class;
    }
}

abstract class MakerEvent<T as Maker> {}

final class GlassMakerEvent extends MakerEvent<GlassMaker> {}