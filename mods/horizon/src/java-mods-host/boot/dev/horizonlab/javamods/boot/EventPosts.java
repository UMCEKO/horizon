package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import dev.horizonlab.javamods.boot.mixin.BlockSnapshotAccess;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;
import java.lang.reflect.Field;
import java.util.HashSet;
import java.util.Set;
import net.minecraft.advancements.AdvancementHolder;
import net.minecraft.advancements.AdvancementProgress;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.BonemealSource;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.gamerules.GameRule;
import net.minecraft.world.level.gamerules.GameRuleMap;
import net.minecraft.world.level.gamerules.GameRules;
import net.minecraft.world.level.storage.LevelData;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.item.ItemEntity;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.event.entity.item.ItemTossEvent;
import net.minecraft.world.level.block.Block;
import net.minecraft.server.level.ChunkLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.level.Explosion;
import net.minecraft.world.level.ServerExplosion;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.level.Level;
import net.neoforged.bus.ListenerList;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.ICancellableEvent;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.entity.player.AdvancementEvent.AdvancementProgressEvent.ProgressType;
import net.neoforged.neoforge.common.util.BlockSnapshot;

/** NeoForge events Rust posts where its hook fires (`game/java/java_listener.rs`): which ones a mod listens to, and the post ops. */
public final class EventPosts {
    private EventPosts() {}

    /** A client event's adapter: reads its fields after the name, posts it, and answers what Rust writes back. */
    interface ClientPost {
        byte[] post(DataInputStream in) throws IOException;
    }

    /** What a server event's listeners left: whether one cancelled it, and the fields its site reads back. */
    record Posted(boolean canceled, byte[] result) {
        static final Posted NONE = new Posted(false, new byte[0]);
    }

    /** A server event posted on the acting player's view: reads its fields after the view and posts it. */
    interface PlayerPost {
        Posted post(GameBridge.Call call) throws Exception;
    }

    /** A server event posted on a level with no acting player: reads its fields after the name and posts it. */
    interface LevelPost {
        Posted post(ServerLevel level, Crossing.In in) throws Exception;
    }

    /** The server events Rust posts through `post_event`, each with the acting player's view filled. */
    private static final Map<String, PlayerPost> PLAYER = new java.util.HashMap<>();
    static {
        PLAYER.put("BreakBlockEvent", EventPosts::breakBlock);
        PLAYER.put("AttackEntityEvent", EventPosts::attackEntity);
        PLAYER.put("BlockEvent.FarmlandTrampleEvent", EventPosts::playerTrample);
        PLAYER.put("BonemealEvent", EventPosts::playerBonemeal);
        PLAYER.put("PlayerSetSpawnEvent", EventPosts::setSpawn);
        PLAYER.put("PlayerWakeUpEvent", EventPosts::wakeUp);
        PLAYER.put("AdvancementEvent.AdvancementEarnEvent", EventPosts::advancementEarn);
        PLAYER.put("AdvancementEvent.AdvancementProgressEvent", EventPosts::advancementProgress);
        PLAYER.put("ItemTossEvent", EventPosts::itemToss);
        PLAYER.put("ArrowLooseEvent", EventPosts::arrowLoose);
        PLAYER.put("BlockEvent.EntityPlaceEvent", EventPosts::place);
        PLAYER.put("PlayerEvent.Clone", EventPosts::playerClone);
        PLAYER.put("PlayerEvent.PlayerRespawnEvent", EventPosts::playerRespawn);
        PLAYER.put("AnvilUpdateEvent", StationEvents::anvilUpdate);
        PLAYER.put("AnvilCraftEvent.Pre", StationEvents::anvilCraftPre);
        PLAYER.put("AnvilCraftEvent.Post", StationEvents::anvilCraftPost);
        PLAYER.put("GrindstoneEvent.OnPlaceItem", StationEvents::grindstonePlace);
        PLAYER.put("GrindstoneEvent.OnTakeItem", StationEvents::grindstoneTake);
        PLAYER.put("EntityTravelToDimensionEvent", EventPosts::travelToDimension);
        PLAYER.put("PlayerEvent.PlayerChangedDimensionEvent", EventPosts::changedDimension);
    }

    /** The server events Rust posts through `post_level_event`. */
    private static final Map<String, LevelPost> LEVEL = Map.of("BlockEvent.FarmlandTrampleEvent", EventPosts::mobTrample, "BonemealEvent", EventPosts::levelBonemeal, "PistonEvent.Pre", EventPosts::pistonPre, "PistonEvent.Post", EventPosts::pistonPost, "GameRuleChangedEvent", EventPosts::gameRuleChanged, "ChunkTicketLevelUpdatedEvent", EventPosts::chunkTicketLevel, "ExplosionEvent.Start", EventPosts::explosionStart, "ExplosionEvent.Detonate", EventPosts::explosionDetonate, "ExplosionKnockbackEvent", EventPosts::explosionKnockback);

    /** The blasts in flight, by Rust's serial: Start, Detonate and every Knockback of one blast share one object. */
    private static final Map<Long, ServerExplosion> EXPLOSIONS = new LinkedHashMap<>(16, 0.75f, true) {
        @Override
        protected boolean removeEldestEntry(Map.Entry<Long, ServerExplosion> eldest) {
            return size() > 8;
        }
    };

    /** The client events Rust posts through `client_post_event`. */
    private static final Map<String, ClientPost> CLIENT = Map.of("PlaySoundEvent", NativeSounds::ask, "ClientChatReceivedEvent", ChatReceived::ask);

    /** The last census: each listened name with its listeners' owners. */
    private static volatile Map<String, List<String>> listened = Map.of();

    /** The packages NeoForge's events live in; a name is `Outer` or `Outer.Inner` within one of them. */
    private static final String[] PACKAGES = {
        "net.neoforged.neoforge.event", "net.neoforged.neoforge.event.entity", "net.neoforged.neoforge.event.entity.living",
        "net.neoforged.neoforge.event.entity.player", "net.neoforged.neoforge.event.entity.item", "net.neoforged.neoforge.event.level",
        "net.neoforged.neoforge.event.level.block", "net.neoforged.neoforge.event.tick", "net.neoforged.neoforge.event.server",
        "net.neoforged.neoforge.event.enchanting", "net.neoforged.neoforge.event.brewing", "net.neoforged.neoforge.event.village",
        "net.neoforged.neoforge.client.event", "net.neoforged.neoforge.client.event.sound", "net.neoforged.neoforge.client.event.lifecycle",
        "net.neoforged.neoforge.network.event", "net.neoforged.neoforge.client.network.event", "net.neoforged.neoforge.registries",
        "net.neoforged.neoforge.registries.datamaps", "net.neoforged.neoforge.server.permission.events",
    };

    /** A listened event: its listeners' owners, a bit per `EventPriority` ordinal with one, and whether one hears it cancelled. */
    record Heard(List<String> owners, int phases, boolean receivesCanceled) {}

    /** `bool loud`, `int n` x (name, `bool sited`), answered with `int m` x (name, `byte phases`, `bool receives canceled`) of the events a loaded mod listens to; `loud` logs each. */
    static byte[] census(byte[] input) throws IOException {
        CompiledEvents.freeze();
        var in = Crossing.In.of(input);
        boolean loud = in.readBoolean();
        int n = in.readInt();
        boolean debug = System.getenv("HORIZON_JAVA_DEBUG") != null;
        var found = new TreeMap<String, Heard>();
        for (int i = 0; i < n; i++) {
            String name = in.text();
            boolean sited = in.readBoolean();
            Class<?> event = resolve(name);
            if (event == null) continue;
            Heard heard = heard(NeoForge.EVENT_BUS, event);
            if (heard.owners().isEmpty()) {
                if (loud && debug) System.err.println("Horizon: server event " + event.getName() + " not listened");
                continue;
            }
            found.put(name, heard);
            if (!loud) continue;
            String by = String.join(", ", heard.owners());
            System.err.println("Horizon: server event " + event.getName() + " listened by " + by);
            if (!sited) System.err.println("Horizon: WARNING: server event " + event.getName() + " is listened (" + by + ") but Horizon has no site for it: mods will not hear it");
        }
        var owners = new TreeMap<String, List<String>>();
        found.forEach((name, heard) -> owners.put(name, heard.owners()));
        listened = Collections.unmodifiableMap(owners);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(found.size());
        for (var entry : found.entrySet()) {
            byte[] utf = entry.getKey().getBytes(StandardCharsets.UTF_8);
            out.writeInt(utf.length);
            out.write(utf);
            out.writeByte(entry.getValue().phases());
            out.writeBoolean(entry.getValue().receivesCanceled());
        }
        return bytes.toByteArray();
    }

    /** `event`'s listeners on the bus beyond Horizon's own, by phase: whom they belong to, their phases, whether one hears a cancelled event. */
    static Heard heard(IEventBus bus, Class<?> event) {
        if (!Event.class.isAssignableFrom(event)) return new Heard(List.of(), 0, false);
        boolean cancellable = ICancellableEvent.class.isAssignableFrom(event);
        var owners = new TreeSet<String>();
        int phases = 0;
        boolean receives = false;
        try {
            for (EventPriority phase : EventPriority.values()) {
                for (var listener : CompiledEvents.phaseListeners(bus, event, phase)) {
                    String owner;
                    try {
                        owner = Listeners.ownerOf(listener).getName();
                    } catch (ReflectiveOperationException | RuntimeException e) {
                        owner = listener.getClass().getName();
                    }
                    if (horizons(owner)) continue;
                    owners.add(owner);
                    phases |= 1 << phase.ordinal();
                    receives |= cancellable && CompiledEvents.receivesCanceled(listener);
                }
            }
        } catch (Throwable e) {
            System.err.println("Horizon: the listeners of " + event.getName() + " can't be read by phase: " + e);
            List<String> all = owners(bus, event);
            return new Heard(all, all.isEmpty() ? 0 : 1 << EventPriority.NORMAL.ordinal(), false);
        }
        return new Heard(List.copyOf(owners), phases, receives);
    }

    /** The last census's listened names, each with its listeners' owners. */
    public static Map<String, List<String>> lastCensus() {
        return listened;
    }

    /** Whether the bus holds a listener of `event` (its own or a supertype's) beyond Horizon's own. */
    static boolean listened(IEventBus bus, Class<?> event) {
        return !owners(bus, event).isEmpty();
    }

    private static Class<?> resolve(String name) {
        String nested = name.replace('.', '$');
        for (String pkg : PACKAGES) {
            try {
                return Class.forName(pkg + "." + nested, false, NeoForge.class.getClassLoader());
            } catch (ClassNotFoundException | LinkageError ignored) {
                // Not in this package, or not on this side.
            }
        }
        System.err.println("Horizon: no NeoForge event class " + name + " on this side; its hooks do not post to Java");
        return null;
    }

    /** Whether a listener's owner is Horizon's own: its listeners never make an event listened, the test mod's do. */
    static boolean horizons(String owner) {
        return owner.startsWith("dev.horizonlab.javamods.") && !owner.startsWith("dev.horizonlab.javamods.testmod.");
    }

    /** The owners of `event`'s listeners on the bus (its own and its supertypes'), sorted and distinct, Horizon's left out. */
    static List<String> owners(IEventBus bus, Class<?> event) {
        if (!Event.class.isAssignableFrom(event)) return List.of();
        var owners = new TreeSet<String>();
        try {
            var list = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            list.setAccessible(true);
            for (var listener : ((ListenerList) list.invoke(bus, event)).getListeners()) {
                String owner;
                try {
                    owner = Listeners.ownerOf(listener).getName();
                } catch (ReflectiveOperationException | RuntimeException e) {
                    owner = listener.getClass().getName();
                }
                if (!horizons(owner)) owners.add(owner);
            }
        } catch (ReflectiveOperationException | RuntimeException e) {
            System.err.println("Horizon: the listeners of " + event.getName() + " can't be read: " + e);
            return List.of();
        }
        return List.copyOf(owners);
    }

    /** `post_event`: the event's name, whether it arrives cancelled, then the acting player's view and its fields; the reply is the posted result, then the use reply's tail. */
    static void server(Crossing.In in, Crossing.Out out) throws Exception {
        String name = in.text();
        boolean arrivesCanceled = in.readBoolean();
        PlayerPost post = PLAYER.get(name);
        if (post == null) throw new IllegalArgumentException("no server post for " + name);
        GameBridge.renewNext = name.equals("PlayerEvent.Clone");
        var call = GameBridge.fill(in, false);
        // A view keeps no use between calls, as in `GameBridge.use`.
        Using.clear(call.player());
        write(out, posted(name, arrivesCanceled, () -> post.post(call)));
        GameBridge.writeUseTail(call.player(), out);
    }

    /** `post_level_event`: the level, the event's name, whether it arrives cancelled, and its fields; the reply is the posted result, then the effects. */
    static void level(Crossing.In in, Crossing.Out out) throws Exception {
        var level = GameBridge.levelOf(in);
        String name = in.text();
        boolean arrivesCanceled = in.readBoolean();
        LevelPost post = LEVEL.get(name);
        if (post == null) throw new IllegalArgumentException("no level post for " + name);
        write(out, posted(name, arrivesCanceled, () -> post.post(level, in)));
        GameBridge.writeEffects(out);
    }

    private interface Step {
        Posted run() throws Exception;
    }

    /** The event classes posts resolved by name. */
    private static final Map<String, Class<?>> CLASSES = new java.util.concurrent.ConcurrentHashMap<>();

    /** Runs a post through `GameBridge.safely`: a listener that throws leaves the event as it was; one a native mod cancelled first starts cancelled. */
    private static Posted posted(String name, boolean arrivesCanceled, Step step) {
        Posted[] posted = {Posted.NONE};
        Runnable run = () -> GameBridge.safely(name, () -> {
            try {
                posted[0] = step.run();
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        });
        if (!arrivesCanceled) {
            run.run();
        } else {
            Class<?> type = CLASSES.computeIfAbsent(name, EventPosts::resolve);
            // Only compiled dispatch can start the site's own event cancelled; without it the cancel stands unheard.
            if (type == null || !CompiledEvents.carry(type, run)) return new Posted(true, Posted.NONE.result());
        }
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event " + name + " canceled=" + posted[0].canceled() + " result=" + posted[0].result().length + " bytes");
        return posted[0];
    }

    private static void write(Crossing.Out out, Posted posted) throws IOException {
        out.writeBoolean(posted.canceled());
        out.writeInt(posted.result().length);
        out.write(posted.result());
    }

    private static Posted breakBlock(GameBridge.Call call) {
        var pos = BlockPos.of(call.in().readLong());
        var player = call.player();
        var level = call.level();
        return new Posted(CommonHooks.fireBlockBreak(level, GameBridge.type(player), player, pos, level.getBlockState(pos)).isCanceled(), Posted.NONE.result());
    }

    private static Posted attackEntity(GameBridge.Call call) {
        var target = call.level().getEntity(call.in().readInt());
        return new Posted(target != null && !CommonHooks.onPlayerAttackTarget(call.player(), target), Posted.NONE.result());
    }

    private static Posted trample(ServerLevel level, BlockPos pos, double distance, Entity entity) {
        boolean canceled = NeoForge.EVENT_BUS.post(new BlockEvent.FarmlandTrampleEvent(level, pos, level.getBlockState(pos), distance, entity)).isCanceled();
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event BlockEvent.FarmlandTrampleEvent " + pos.toShortString() + " canceled=" + canceled);
        return new Posted(canceled, Posted.NONE.result());
    }

    /** Posted, not `CommonHooks.onFarmlandTrample`: its `canTrample` draw already happened natively. */
    private static Posted playerTrample(GameBridge.Call call) {
        var pos = BlockPos.of(call.in().readLong());
        return trample(call.level(), pos, call.in().readDouble(), call.player());
    }

    private static Posted mobTrample(ServerLevel level, Crossing.In in) {
        var pos = BlockPos.of(in.readLong());
        double distance = in.readDouble();
        var mob = EntityViews.byId(in.readInt());
        if (mob == null) throw new IllegalStateException("no Java view of the mob that trampled " + pos);
        return trample(level, pos, distance, mob);
    }

    private static Posted bonemeal(Player player, ServerLevel level, BlockPos pos, ItemStack stack) {
        var event = EventHooks.fireBonemealEvent(player, level, pos, level.getBlockState(pos), BonemealSource.INTERACTION, stack);
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event BonemealEvent " + pos.toShortString() + " canceled=" + event.isCanceled() + " successful=" + event.isSuccessful());
        return new Posted(event.isCanceled(), new byte[] {(byte) (event.isSuccessful() ? 1 : 0)});
    }

    private static Posted playerBonemeal(GameBridge.Call call) {
        var pos = BlockPos.of(call.in().readLong());
        return bonemeal(call.player(), call.level(), pos, call.player().getItemInHand(call.hand()));
    }

    private static Posted levelBonemeal(ServerLevel level, Crossing.In in) throws IOException {
        var pos = BlockPos.of(in.readLong());
        byte[] tag = in.readNBytes(in.readInt());
        return bonemeal(null, level, pos, tag.length == 0 ? ItemStack.EMPTY : GameBridge.decode(tag));
    }

    private static Posted pistonPre(ServerLevel level, Crossing.In in) {
        var pos = BlockPos.of(in.readLong());
        var direction = Direction.from3DDataValue(in.readByte());
        boolean extend = in.readBoolean();
        boolean canceled = EventHooks.onPistonMovePre(level, pos, direction, extend);
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event PistonEvent.Pre " + pos + " " + direction + " extend=" + extend + " canceled=" + canceled);
        return new Posted(canceled, Posted.NONE.result());
    }

    private static Posted pistonPost(ServerLevel level, Crossing.In in) {
        var pos = BlockPos.of(in.readLong());
        var direction = Direction.from3DDataValue(in.readByte());
        boolean extend = in.readBoolean();
        EventHooks.onPistonMovePost(level, pos, direction, extend);
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event PistonEvent.Post " + pos + " " + direction + " extend=" + extend);
        return Posted.NONE;
    }

    private static void debug(String line) {
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event " + line);
    }

    /** `bool present`, then if present `text dimension`, `long pos`, `float yaw`, `float pitch`, `bool forced`; canceled when a listener vetoes the point. */
    private static Posted setSpawn(GameBridge.Call call) {
        var in = call.in();
        ServerPlayer.RespawnConfig config = null;
        if (in.readBoolean()) {
            var dimension = ResourceKey.create(Registries.DIMENSION, Identifier.parse(in.text()));
            var pos = BlockPos.of(in.readLong());
            float yaw = in.readFloat();
            float pitch = in.readFloat();
            config = new ServerPlayer.RespawnConfig(LevelData.RespawnData.of(dimension, pos, yaw, pitch), in.readBoolean());
        }
        boolean canceled = EventHooks.onPlayerSpawnSet(call.player(), config);
        debug("PlayerSetSpawnEvent present=" + (config != null) + " canceled=" + canceled);
        return new Posted(canceled, Posted.NONE.result());
    }

    private static Posted wakeUp(GameBridge.Call call) {
        boolean immediately = call.in().readBoolean();
        boolean updateLevel = call.in().readBoolean();
        EventHooks.onPlayerWakeup(call.player(), immediately, updateLevel);
        debug("PlayerWakeUpEvent " + immediately + " " + updateLevel);
        return Posted.NONE;
    }

    private static final Set<String> UNKNOWN_ADVANCEMENTS = new HashSet<>();

    /** The data packs' own holder for `id`; an id they don't know (a Horizon-only advancement) is logged once and skipped. */
    private static AdvancementHolder holder(String id) {
        var holder = GameBridge.server().getAdvancements().get(Identifier.parse(id));
        if (holder == null && UNKNOWN_ADVANCEMENTS.add(id)) System.err.println("Horizon: advancement " + id + " is not in Java's manager; its events are not posted");
        return holder;
    }

    private static Posted advancementEarn(GameBridge.Call call) {
        var holder = holder(call.in().text());
        if (holder == null) return Posted.NONE;
        EventHooks.onAdvancementEarnedEvent(call.player(), holder);
        debug("AdvancementEvent.AdvancementEarnEvent " + holder.id());
        return Posted.NONE;
    }

    /** `text id`, `text criterion`, `byte type` (0 grant, 1 revoke), `int n` x `text` done criteria: native's progress after the change. */
    private static Posted advancementProgress(GameBridge.Call call) {
        var in = call.in();
        var holder = holder(in.text());
        String criterion = in.text();
        var type = in.readByte() == 0 ? ProgressType.GRANT : ProgressType.REVOKE;
        var done = new java.util.ArrayList<String>();
        for (int i = in.readInt(); i > 0; i--) done.add(in.text());
        if (holder == null) return Posted.NONE;
        var progress = new AdvancementProgress();
        progress.update(holder.value().requirements());
        done.forEach(progress::grantProgress);
        EventHooks.onAdvancementProgressedEvent(call.player(), holder, progress, criterion, type);
        debug("AdvancementEvent.AdvancementProgressEvent " + holder.id() + " " + criterion + " " + type);
        return Posted.NONE;
    }

    private static volatile Field ruleValues;

    /** Writes `value` into `rules` without the change callback `GameRules.set` runs. */
    private static <T> void store(GameRules rules, GameRule<T> rule, T value) throws ReflectiveOperationException {
        Field field = ruleValues;
        if (field == null) {
            for (var candidate : GameRules.class.getDeclaredFields()) if (candidate.getType() == GameRuleMap.class) field = candidate;
            if (field == null) throw new NoSuchFieldException("GameRules holds no GameRuleMap");
            field.setAccessible(true);
            ruleValues = field;
        }
        ((GameRuleMap) field.get(rules)).set(rule, value);
    }

    private static <T> void ruleChanged(MinecraftServer server, GameRule<T> rule, String text) throws ReflectiveOperationException {
        T value = rule.deserialize(text).getOrThrow(IllegalStateException::new);
        var global = server.getGlobalGameRules();
        store(global, rule, value);
        if (server.getGameRules() != global) store(server.getGameRules(), rule, value);
        EventHooks.onGameRuleChanged(server, rule, value);
        debug("GameRuleChangedEvent " + rule.id() + "=" + text);
    }

    /** `text rule id`, `text value`: the new value is stored in Java's rules first, so `event.getGameRules()` answers it. */
    private static Posted gameRuleChanged(ServerLevel level, Crossing.In in) throws Exception {
        String id = in.text();
        String value = in.text();
        var rule = BuiltInRegistries.GAME_RULE.getValue(Identifier.parse(id));
        if (rule == null) {
            System.err.println("Horizon: game rule " + id + " is not in Java's registry; its change is not posted");
            return Posted.NONE;
        }
        ruleChanged(level.getServer(), rule, value);
        return Posted.NONE;
    }

    private static Posted intResult(boolean canceled, int value) {
        var bytes = new ByteArrayOutputStream();
        try {
            new DataOutputStream(bytes).writeInt(value);
        } catch (IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
        return new Posted(canceled, bytes.toByteArray());
    }

    /** `Player.drop`: the item built from Rust's position and motion, posted, and spawned from the event's entity; the result is its id. */
    private static Posted itemToss(GameBridge.Call call) throws Exception {
        var in = call.in();
        var stack = EntityViews.stack(in.readNBytes(in.readInt()), call.level());
        double x = in.readDouble(), y = in.readDouble(), z = in.readDouble();
        double vx = in.readDouble(), vy = in.readDouble(), vz = in.readDouble();
        int delay = in.readInt();
        boolean named = in.readBoolean();
        var item = new ItemEntity(call.level(), x, y, z, stack, vx, vy, vz);
        item.setPickUpDelay(delay);
        if (named) item.setThrower(call.player());
        var event = new ItemTossEvent(item, call.player());
        boolean canceled = NeoForge.EVENT_BUS.post(event).isCanceled();
        int id = -1;
        if (!canceled && call.level().addFreshEntity(event.getEntity())) id = event.getEntity().getId();
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event ItemTossEvent canceled=" + canceled + " id=" + id);
        return intResult(canceled, id);
    }

    /** `BowItem.releaseUsing` and `CrossbowItem.performShooting`: the charge the listeners left, negative when they cancelled. */
    private static Posted arrowLoose(GameBridge.Call call) {
        var in = call.in();
        var hand = in.readInt() == 0 ? InteractionHand.MAIN_HAND : InteractionHand.OFF_HAND;
        int charge = in.readInt();
        boolean hasAmmo = in.readBoolean();
        boolean crossbow = in.readBoolean();
        var stack = call.player().getItemInHand(hand);
        int out = crossbow ? EventHooks.onArrowLoose(stack, call.level(), call.player(), 1) : EventHooks.onArrowLoose(stack, call.level(), call.player(), charge, hasAmmo);
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event ArrowLooseEvent " + charge + "->" + out);
        return intResult(false, out);
    }

    /** `CommonHooks.onPlaceItemIntoWorld`'s event over Rust's captured writes: `byte face`, `int n` x (`long pos`, `int oldState`, `int flags`). */
    private static Posted place(GameBridge.Call call) {
        var in = call.in();
        var face = Direction.from3DDataValue(in.readByte());
        int n = in.readInt();
        var level = call.level();
        var snapshots = new ArrayList<BlockSnapshot>(n);
        for (int i = 0; i < n; i++) {
            var pos = BlockPos.of(in.readLong());
            var old = Block.stateById(in.readInt());
            snapshots.add(BlockSnapshotAccess.horizon$create(level.dimension(), level, pos, old, null, in.readInt()));
        }
        boolean canceled = n > 1 ? EventHooks.onMultiBlockPlace(call.player(), snapshots, face) : n == 1 && EventHooks.onBlockPlace(call.player(), snapshots.get(0), face);
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event BlockEvent.EntityPlaceEvent n=" + n + " canceled=" + canceled);
        return new Posted(canceled, Posted.NONE.result());
    }

    /** The level of a chunk no ticket reaches (`ChunkLevel.MAX_LEVEL + 1`). */
    private static final int UNLOADED_LEVEL = ChunkLevel.MAX_LEVEL + 1;

    /** `int x`, `int z`, `int old`, `int new` of a chunk's ticket level, posted with the holder it had and, once unloaded, loses. */
    private static Posted chunkTicketLevel(ServerLevel level, Crossing.In in) {
        int x = in.readInt(), z = in.readInt(), old = in.readInt(), now = in.readInt();
        var holder = ChunkViews.holder(level, x, z, old, now);
        EventHooks.fireChunkTicketLevelUpdated(level, ChunkPos.pack(x, z), old, now, holder);
        if (now >= UNLOADED_LEVEL) ChunkViews.dropHolder(level, x, z);
        return Posted.NONE;
    }

    /** Reads a blast's header and gives its explosion: the one its serial built before, else a new one. */
    private static ServerExplosion explosion(ServerLevel level, Crossing.In in) {
        long serial = in.readLong();
        var center = new Vec3(in.readDouble(), in.readDouble(), in.readDouble());
        float radius = in.readFloat();
        boolean fire = in.readBoolean();
        var interaction = Explosion.BlockInteraction.values()[in.readByte()];
        int source = in.readInt();
        int owner = in.readInt();
        String damageType = in.text();
        var known = EXPLOSIONS.get(serial);
        if (known != null) return known;
        var key = ResourceKey.create(Registries.DAMAGE_TYPE, Identifier.parse(damageType));
        var type = level.registryAccess().lookupOrThrow(Registries.DAMAGE_TYPE).getOrThrow(key);
        Entity sourceView = source < 0 ? null : EntityViews.byId(source);
        Entity ownerView = owner < 0 ? null : EntityViews.byId(owner);
        var made = new ServerExplosion(level, sourceView, new DamageSource(type, sourceView, ownerView), null, center, radius, fire, interaction);
        EXPLOSIONS.put(serial, made);
        return made;
    }

    private static List<BlockPos> blocks(Crossing.In in) {
        int n = in.readInt();
        var blocks = new ArrayList<BlockPos>(n);
        for (int i = 0; i < n; i++) blocks.add(BlockPos.of(in.readLong()));
        return blocks;
    }

    private static Posted explosionStart(ServerLevel level, Crossing.In in) {
        var explosion = explosion(level, in);
        return new Posted(EventHooks.onExplosionStart(level, explosion), Posted.NONE.result());
    }

    /** The blocks and entities as listeners left them, both written back. */
    private static Posted explosionDetonate(ServerLevel level, Crossing.In in) throws IOException {
        var explosion = explosion(level, in);
        var blocks = blocks(in);
        var entities = new ArrayList<Entity>();
        int m = in.readInt();
        for (int i = 0; i < m; i++) entities.add(entityRef(in));
        int before = blocks.size();
        int entitiesBefore = entities.size();
        EventHooks.onExplosionDetonate(level, explosion, entities, blocks);
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event ExplosionEvent.Detonate blocks " + before + "->" + blocks.size() + " entities " + entitiesBefore + "->" + entities.size());
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(blocks.size());
        for (BlockPos pos : blocks) out.writeLong(pos.asLong());
        out.writeInt(entities.size());
        for (Entity entity : entities) {
            if (entity instanceof net.minecraft.server.level.ServerPlayer player) {
                out.writeByte(1);
                out.writeLong(player.getUUID().getMostSignificantBits());
                out.writeLong(player.getUUID().getLeastSignificantBits());
            } else {
                out.writeByte(0);
                out.writeInt(entity.getId());
            }
        }
        return new Posted(false, bytes.toByteArray());
    }

    private static Posted explosionKnockback(ServerLevel level, Crossing.In in) throws IOException {
        var explosion = explosion(level, in);
        var entity = entityRef(in);
        var push = new Vec3(in.readDouble(), in.readDouble(), in.readDouble());
        var pushed = EventHooks.getExplosionKnockback(level, explosion, entity, push, blocks(in));
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeDouble(pushed.x);
        out.writeDouble(pushed.y);
        out.writeDouble(pushed.z);
        return new Posted(false, bytes.toByteArray());
    }

    /** An entity ref: `byte 0` and an id (a native entity), or `byte 1` and a uuid (a player). */
    private static Entity entityRef(Crossing.In in) {
        boolean player = in.readByte() == 1;
        Entity view;
        String ref;
        if (player) {
            var uuid = new java.util.UUID(in.readLong(), in.readLong());
            view = GameBridge.playerView(uuid);
            ref = "player " + uuid;
        } else {
            int id = in.readInt();
            view = EntityViews.byId(id);
            ref = "id " + id;
        }
        if (view == null) throw new IllegalStateException("Horizon: no view for entity " + ref);
        return view;
    }

    /** `PlayerList.respawn`'s `restoreFrom`: the new view and the old one, which NeoForge's own listener copies attachments between. */
    private static Posted playerClone(GameBridge.Call call) throws IOException {
        boolean wasDeath = call.in().readBoolean();
        var original = GameBridge.original;
        try {
            if (original != null) {
                debug("PlayerEvent.Clone wasDeath=" + wasDeath + " same=" + (original == call.player()));
                EventHooks.onPlayerClone(call.player(), original, wasDeath);
            }
        } finally {
            GameBridge.releaseOriginal();
        }
        return Posted.NONE;
    }

    private static Posted playerRespawn(GameBridge.Call call) throws IOException {
        boolean endConquered = call.in().readBoolean();
        debug("PlayerEvent.PlayerRespawnEvent endConquered=" + endConquered);
        EventHooks.firePlayerRespawnEvent(call.player(), endConquered);
        return Posted.NONE;
    }

    private static ResourceKey<Level> dimension(Crossing.In in) {
        return ResourceKey.create(Registries.DIMENSION, Identifier.parse(GameBridge.text(in)));
    }

    private static Posted travelToDimension(GameBridge.Call call) {
        var to = dimension(call.in());
        boolean canceled = !CommonHooks.onTravelToDimension(call.player(), to);
        debug("EntityTravelToDimensionEvent to=" + to.identifier() + " canceled=" + canceled);
        return new Posted(canceled, Posted.NONE.result());
    }

    private static Posted changedDimension(GameBridge.Call call) {
        var from = dimension(call.in());
        var to = dimension(call.in());
        debug("PlayerEvent.PlayerChangedDimensionEvent " + from.identifier() + " -> " + to.identifier());
        EventHooks.firePlayerChangedDimensionEvent(call.player(), from, to);
        return Posted.NONE;
    }

    /** `client_post_event`: the event's name, then its fields, posted on the Java client's thread. */
    static byte[] client(DataInputStream in) throws IOException {
        String name = new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
        ClientPost post = CLIENT.get(name);
        if (post == null) throw new IllegalArgumentException("no client post for " + name);
        return post.post(in);
    }
}
