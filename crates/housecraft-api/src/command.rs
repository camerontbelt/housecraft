use std::collections::BTreeMap;

use housecraft_model::*;
use housecraft_units::{DisplayUnit, Length, Point2, Point3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::convert::CadTarget;
use crate::ApiError;
use crate::ops::{self, expect, invalid, positive};

/// How far the material painter reaches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PaintScope {
    /// The chosen objects.
    #[default]
    Object,
    /// Everything on the chosen floors.
    Floor,
    /// The whole plan (the default set's material for the surface).
    Plan,
}

/// How [`Command::CadBoolean`] combines shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ShapeOp {
    /// Join them into one outline.
    Union,
    /// Keep only where they all overlap.
    Intersect,
    /// Cut the others out of the first.
    Subtract,
}

/// One side of a lot in [`Command::AddPlotLines`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlotLeg {
    /// A surveyor's bearing, e.g. "N 45°30'15\" E".
    pub bearing: String,
    pub distance: Length,
}

/// Text for [`Command::AddCadDetail`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DetailText {
    pub position: Point2,
    pub text: String,
    pub height: Option<Length>,
    #[serde(default)]
    pub rotation: f64,
    #[serde(default)]
    pub justify: Justify,
}

/// A single edit to a document. Serialized with an `"op"` tag, e.g.
/// `{"op": "add_wall", "start": [0, 0], "end": ["20'", 0]}`.
///
/// Objects created by `add_*` commands take their style, size and layer from
/// the active default set unless the command says otherwise. Any property of
/// any object can be changed with `update`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Command {
    /// Rename the document.
    SetDocumentName { name: String },
    /// Change how lengths are displayed.
    SetDisplayUnit { unit: DisplayUnit },
    /// Add a floor. By default it goes on top: its elevation is the highest
    /// floor's ceiling plus this floor's structure. With `below`, it is inserted
    /// under that floor and every floor from there up moves up to make room.
    AddFloor {
        name: Option<String>,
        elevation: Option<Length>,
        /// Defaults to the highest floor's ceiling height, or 8'.
        ceiling_height: Option<Length>,
        /// Floor platform depth (subfloor + joists); default 12 5/8".
        #[serde(default)]
        floor_structure: Option<Length>,
        #[serde(default)]
        below: Option<Id>,
    },
    /// Build (or rebuild) the foundation under the lowest floor from its
    /// exterior walls. Omitted sizes use typical values for the kind.
    BuildFoundation {
        kind: FoundationKind,
        wall_height: Option<Length>,
        wall_thickness: Option<Length>,
        footing_width: Option<Length>,
        footing_depth: Option<Length>,
        slab_thickness: Option<Length>,
        /// Keep the foundation in step with the floor above (default true).
        #[serde(default = "yes")]
        auto_rebuild: bool,
        /// Pier foundations: spacing along the beams (default 6').
        pier_spacing: Option<Length>,
        /// Pier foundations: pier diameter (default 12").
        pier_diameter: Option<Length>,
        /// Pier foundations: square piers.
        #[serde(default)]
        square_piers: bool,
        /// Slabs: a curb this high round garages.
        #[serde(default)]
        garage_curb: Option<Length>,
    },
    /// Build a roof over a floor's exterior walls (default: the top storey).
    /// Each wall's `roof` settings choose gable or hip, pitch and overhang.
    /// With `auto_rebuild` the roof follows later wall changes.
    BuildRoof {
        floor: Option<Id>,
        #[serde(default = "yes")]
        auto_rebuild: bool,
    },
    /// Set every exterior wall's roof edge for a whole-roof style (gable ends
    /// go on the short sides), then build the roof and keep it automatic.
    /// `lower_pitch` and `break_height` shape mansards and gambrels.
    SetRoofStyle {
        floor: Option<Id>,
        style: RoofStyle,
        pitch: Option<f64>,
        lower_pitch: Option<f64>,
        break_height: Option<Length>,
    },
    /// Delete a floor's roof planes and stop rebuilding them.
    RemoveRoof { floor: Option<Id> },
    /// Add a hand-drawn roof plane: `points` outline it in plan; it rises at
    /// `pitch` degrees from the line `eave_a`–`eave_b`, which sits
    /// `eave_height` above the floor.
    AddRoofPlane {
        floor: Option<Id>,
        points: Vec<Point2>,
        eave_a: Point2,
        eave_b: Point2,
        eave_height: Length,
        pitch: f64,
    },
    /// Add a stair from a floor up to the floor above. Its risers come from the
    /// floor-to-floor height (or `rise`) and the default maximum riser. For L
    /// and U shapes, `first_treads: 0` splits the flights evenly.
    AddStair {
        floor: Option<Id>,
        /// Centre of the bottom riser.
        start: Point2,
        /// Direction of travel going up, degrees counter-clockwise from east.
        direction: f64,
        #[serde(default = "straight")]
        shape: StairShape,
        width: Option<Length>,
        rise: Option<Length>,
        railing_left: Option<bool>,
        railing_right: Option<bool>,
        /// Cut an opening in the floor above (default true).
        #[serde(default = "yes")]
        stairwell: bool,
    },
    /// Place an electrical device or fixture. Wall devices (outlets,
    /// switches, wall lights) snap to the nearest wall face within 2' and
    /// face into the room unless `rotation` is given. An outlet with no wall
    /// that near becomes a floor outlet; one facing outdoors is weatherproof.
    AddElectrical {
        floor: Option<Id>,
        electrical_kind: ElectricalKind,
        at: Point2,
        rotation: Option<f64>,
        height: Option<Length>,
        /// Fixtures a switch controls.
        #[serde(default)]
        controls: Vec<Id>,
    },
    /// Auto Stairwell: cut the floor above the stair and put railings round
    /// the opening there, leaving the top end open as the way off the stair.
    AutoStairwell { stair: Id },
    /// Wire a switch to fixtures (or unwire with `disconnect`). Switches that
    /// share a fixture become 3-way and 4-way automatically.
    ConnectSwitch {
        switch: Id,
        fixtures: Vec<Id>,
        #[serde(default)]
        disconnect: bool,
    },
    /// Mark walls, doors, windows and electrical as existing, new or to be
    /// demolished. Other kinds in `ids` are rejected.
    SetRenovation { ids: Vec<Id>, status: Renovation },
    /// Orient the plan: true north as an angle counter-clockwise from +x
    /// (90 = up), and where to draw the north pointer (`null` hides it).
    SetNorth { angle: f64, pointer: Option<Point2> },
    /// Position the sun for 3D: give `azimuth` / `altitude` directly, or
    /// `latitude`, `day_of_year` and solar `hour` to compute them.
    SetSun {
        azimuth: Option<f64>,
        altitude: Option<f64>,
        latitude: Option<f64>,
        day_of_year: Option<u32>,
        hour: Option<f64>,
        shadows: Option<bool>,
    },
    /// Place a schedule table (door, window, room, wall, fixture, stair or
    /// roof plane) with its top-left corner at `position`.
    AddSchedule {
        floor: Option<Id>,
        schedule_kind: ScheduleKind,
        position: Point2,
        #[serde(default)]
        columns: Vec<String>,
        #[serde(default)]
        all_floors: bool,
    },
    /// Set (or with `null`, clear) the unit price of a materials list item,
    /// keyed "Category: Item" as the materials list shows them.
    SetMaterialPrice { item: String, price: Option<f64> },
    /// Markup percentage added to the materials list total.
    SetMarkup { percent: f64 },
    /// Merge unit prices (e.g. from the master price list) into the plan's;
    /// `overwrite` replaces prices the plan already has.
    ImportPrices {
        prices: std::collections::BTreeMap<String, f64>,
        #[serde(default)]
        overwrite: bool,
    },
    /// Set (or with `null`, clear) object `id`'s own materials list lines.
    SetComponents { id: Id, components: Option<ObjectComponents> },
    /// Add an elevation or cross-section view looking across the line `a`→`b`
    /// toward its left side.
    AddSectionView {
        name: Option<String>,
        floor: Option<Id>,
        a: Point2,
        b: Point2,
        #[serde(default)]
        section_kind: SectionKind,
        depth: Option<Length>,
        #[serde(default)]
        clip_sides: bool,
    },
    /// Add North, South, East and West exterior elevations around the building.
    AutoElevations { floor: Option<Id> },
    /// Add four interior elevations of a room, one looking at each side.
    RoomElevations { room: Id },
    /// Create a layout (default 36" x 24" sheets, one page).
    AddLayout {
        name: String,
        paper_width: Option<Length>,
        paper_height: Option<Length>,
        #[serde(default = "one")]
        pages: u32,
    },
    /// Add a page to a layout.
    AddLayoutPage { layout: Id, name: Option<String> },
    /// Rename `layout`'s pages `{prefix}{n}` from `start` (A-1, A-2…).
    NumberPages {
        layout: Id,
        #[serde(default = "page_prefix")]
        prefix: String,
        #[serde(default = "one")]
        start: u32,
    },
    /// Delete the boxes on `layout` (one page, or all when `page` is
    /// `None`) that show `kind` ("plan", "view", "detail", "schedule",
    /// "materials_list", "revisions", "sheet_index", "text"), or every box.
    DeleteLayoutBoxes {
        layout: Id,
        page: Option<u32>,
        kind: Option<String>,
    },
    /// Freeze layout box `id` into a line drawing: a new CAD detail holding
    /// `shapes` and `texts` (the box's drawing as plain linework, which can
    /// then be edited), shown in the box at the scale it had (`scale`).
    ConvertBoxToLines {
        id: Id,
        scale: f64,
        #[serde(default)]
        shapes: Vec<DetailShape>,
        #[serde(default)]
        texts: Vec<DetailText>,
    },
    /// Client, designer and custom project fields.
    SetProjectInfo { info: ProjectInfo },
    /// Break a wall (or a framing member) in two at the point on it nearest
    /// `at` (Add Break): doors and windows go with the piece they are in.
    BreakWall { id: Id, at: Point2 },
    /// Move `wall` along a dimension so that segment `segment` (0 = the
    /// first pair of points) measures `length`, as when you click a
    /// dimension next to a selected wall and type a new value. Walls joined
    /// to it stretch to follow.
    MoveWallByDimension { dimension: Id, segment: usize, wall: Id, length: Length },
    /// Move objects by typing on a dimension: point `moving` of the
    /// dimension (an end of `segment`, on the objects) ends up `length` from
    /// the segment's other end, the objects travel with it along the
    /// dimension, and so does that point of a manual dimension. A lone wall
    /// drags the walls joined to it.
    MoveByDimension { dimension: Id, segment: usize, moving: usize, ids: Vec<Id>, length: Length },
    /// Point to Point Move: move objects so that `from` (a point on them)
    /// lands on `to`; with `copy`, a moved copy is made instead.
    PointToPointMove {
        ids: Vec<Id>,
        from: Point2,
        to: Point2,
        #[serde(default)]
        copy: bool,
    },
    /// Reflect About Object: mirror objects across a wall or CAD line, or
    /// across the centre line of a door or window (square to its wall).
    /// With `copy`, the originals stay and mirrored copies are made.
    ReflectAbout {
        ids: Vec<Id>,
        about: Id,
        #[serde(default)]
        copy: bool,
    },
    /// Make Parallel / Perpendicular: rotate objects about the middle of
    /// `edge` (one of their edges) so it lies parallel to `reference`, or
    /// square to it when it is already closer to square.
    MakeParallel { ids: Vec<Id>, edge: [Point2; 2], reference: [Point2; 2] },
    /// Record time spent on the plan (dated today unless given).
    LogTime {
        task: String,
        minutes: f64,
        #[serde(default)]
        note: String,
        date: Option<String>,
    },
    /// Remove time log entry `index` (every entry when `None`).
    DeleteTime { index: Option<usize> },
    /// Pin (or with `pinned: false`, unpin) objects in the Project Browser.
    PinObjects { ids: Vec<Id>, pinned: bool },
    /// Add (`index` null) or replace one of the plan's hatch patterns.
    SetHatchPattern { index: Option<u32>, pattern: HatchPattern },
    /// Delete hatch pattern `index`; fills using it become plain hatching.
    DeleteHatchPattern { index: u32 },
    /// Turn CAD shapes into building objects (the CAD is removed).
    ConvertCad { ids: Vec<Id>, to: CadTarget },
    /// Trace roof planes as closed CAD polylines in plan (the planes stay).
    RoofPlanesToPolylines { ids: Vec<Id> },
    /// Turn a straight, L or U stair into flights and landings that can be
    /// edited one by one (widths, landing depths, splits).
    BreakStair { id: Id },
    /// Cabinets on `floor` (every floor when `None`) get end panels on
    /// sides that are not against another cabinet or a wall.
    AutoEndPanels { floor: Option<Id> },
    /// Beams and posts under the floor above room `room`: beams along the
    /// room's long way at most `max_span` apart, posts at most
    /// `post_spacing` apart (kept when framing is rebuilt).
    AutoPostAndBeam { room: Id, max_span: Option<Length>, post_spacing: Option<Length> },
    /// Scatter `count` copies of plant, symbol or block `source` at random
    /// inside closed CAD shape `region` (or along it, when open), turned
    /// at random and sized up to `size_variation` (0–1) either way.
    Scatter {
        source: Id,
        region: Id,
        count: u32,
        #[serde(default)]
        seed: u64,
        #[serde(default)]
        size_variation: f64,
        #[serde(default = "yes")]
        rotate: bool,
    },
    /// Set (or with `null`, delete) user text macro `name`.
    SetMacro { name: String, text: Option<String> },
    /// Replace `find` with `replace` in text, callouts, room names, object
    /// labels and layout notes (`ids`: only those objects).
    ReplaceText {
        find: String,
        replace: String,
        #[serde(default)]
        match_case: bool,
        #[serde(default)]
        ids: Vec<Id>,
    },
    /// Every text style using font `from` uses `to` instead.
    ReplaceFont { from: String, to: String },
    /// Record a revision (numbered after the last; dated today unless given).
    AddRevision {
        description: String,
        #[serde(default)]
        by: String,
        date: Option<String>,
    },
    /// Remove revision `number`.
    DeleteRevision { number: u32 },
    /// Stamp text across every layout page (`None`: no watermark).
    SetWatermark { watermark: Option<Watermark> },
    /// Place a view on a layout page. Position and size are paper
    /// millimetres from the sheet's bottom-left; omitted ones fill the next
    /// free quarter of the page. `scale` 0 or omitted fits a standard scale.
    SendToLayout {
        layout: Id,
        #[serde(default)]
        page: u32,
        source: LayoutSource,
        x: Option<Length>,
        y: Option<Length>,
        width: Option<Length>,
        height: Option<Length>,
        scale: Option<f64>,
        label: Option<String>,
    },
    /// Add a note with an optional leader arrow to `target`, or a marker
    /// (the text inside a circle, hexagon, square or diamond).
    AddCallout {
        floor: Option<Id>,
        text: String,
        at: Point2,
        target: Option<Point2>,
        marker: Option<MarkerShape>,
    },
    /// Add a cabinet. Unset sizes come from the default set's cabinet
    /// defaults. Without `facing`, a cabinet clicked near a wall backs onto
    /// it and faces into the room (`at` then snaps to the wall face).
    AddCabinet {
        floor: Option<Id>,
        cabinet_kind: CabinetKind,
        at: Point2,
        facing: Option<f64>,
        width: Option<Length>,
        depth: Option<Length>,
        height: Option<Length>,
        #[serde(default)]
        shape: CabinetShape,
        /// A sink base: false front and a sink cut into its countertop.
        #[serde(default)]
        sink: bool,
    },
    /// Add a countertop of any outline, with optional holes; its top sits at
    /// `height` (default: base cabinet height plus thickness).
    AddCountertop {
        floor: Option<Id>,
        points: Vec<Point2>,
        height: Option<Length>,
        thickness: Option<Length>,
        edge: Option<EdgeProfile>,
        #[serde(default)]
        holes: Vec<CounterHole>,
    },
    /// Add an appliance or plumbing fixture at its typical size; it backs
    /// onto a nearby wall like a cabinet.
    AddAppliance {
        floor: Option<Id>,
        appliance_kind: ApplianceKind,
        at: Point2,
        facing: Option<f64>,
        width: Option<Length>,
        depth: Option<Length>,
        height: Option<Length>,
    },
    /// Place a cabinet or appliance from a template (a library item): its
    /// sizes and options come from `template`, its place from `at` and
    /// `facing` (backed onto a nearby wall when `facing` is unset).
    PlaceObject {
        floor: Option<Id>,
        kind: ObjectKind,
        template: serde_json::Value,
        at: Point2,
        facing: Option<f64>,
    },
    /// Replace cabinets or appliances with a template (a library item),
    /// keeping where they are. With `all_identical`, every object identical
    /// to one of `ids` (apart from where it is) is replaced too.
    ReplaceWith {
        ids: Vec<Id>,
        template: serde_json::Value,
        #[serde(default)]
        all_identical: bool,
    },
    /// Place an image on a floor, view or CAD detail; `height` defaults to
    /// keeping `pixels` (width, height) in proportion.
    AddPicture {
        floor: Option<Id>,
        path: String,
        position: Point2,
        width: Length,
        height: Option<Length>,
        pixels: Option<[u32; 2]>,
        transparent: Option<Color>,
    },
    /// Store an imported 3D model in the plan (see `housecraft-io-native`'s
    /// model readers); place copies with `add_symbol`.
    AddSymbolDef {
        name: String,
        positions: Vec<[f32; 3]>,
        triangles: Vec<[u32; 3]>,
        #[serde(default)]
        groups: Vec<SymbolGroup>,
        #[serde(default)]
        behavior: SymbolBehavior,
    },
    /// Place a door or window from a library template (an opening's
    /// properties without its wall, place or id) at the point on `wall`
    /// nearest `at`.
    PlaceOpening {
        wall: Id,
        at: Point2,
        template: serde_json::Value,
    },
    /// Use CAD lines (drawn anywhere, at any size) as a window's grille:
    /// their outline is stretched over the glass.
    MuntinsFromCad { opening: Id, ids: Vec<Id> },
    /// Make the selected objects one 3D symbol (they are replaced by a
    /// placed copy of it).
    ConvertToSymbol { ids: Vec<Id>, name: String },
    /// Place a 3D symbol at its imported size; it backs onto a nearby wall
    /// when `facing` is unset (wall-hung symbols at their hanging height).
    AddSymbol {
        floor: Option<Id>,
        def: Id,
        at: Point2,
        facing: Option<f64>,
        elevation: Option<Length>,
    },
    /// Add a backsplash along a path (it stands to the right of the path's
    /// direction, so draw it along the wall with the room on the right);
    /// unset sizes come from the cabinet defaults.
    AddBacksplash {
        floor: Option<Id>,
        points: Vec<Point2>,
        bottom: Option<Length>,
        height: Option<Length>,
        thickness: Option<Length>,
    },
    /// Add a custom molding profile from (out, up) points in mm, or from a
    /// closed CAD polyline (drawn anywhere: its shape is used, not its place).
    AddMoldingProfile {
        name: String,
        #[serde(default)]
        points: Vec<[f64; 2]>,
        from_cad: Option<Id>,
    },
    /// Add a molding along a path; profiles in `stack` sit one on another.
    AddMoldingPolyline {
        floor: Option<Id>,
        points: Vec<Point2>,
        #[serde(default)]
        closed: bool,
        #[serde(default)]
        elevation: Length,
        stack: Vec<MoldingSpec>,
    },
    /// Dimension each run of base and tall cabinets along its front, ends
    /// and every cabinet (the NKBA way).
    AutoCabinetDimensions { floor: Option<Id> },
    /// Close gaps up to the default set's largest filler between side-by-side
    /// cabinets and between cabinets and walls on a floor (cabinets marked
    /// `no_auto_filler` are skipped).
    AutoFillers { floor: Option<Id> },
    /// Add a material. Unset properties come from `copy_from`, or suit the class.
    AddMaterial {
        name: String,
        category: Option<String>,
        class: Option<MaterialClass>,
        color: Option<Color>,
        copy_from: Option<Id>,
    },
    /// Material painter. `scope` object: paint `surface` (every surface when
    /// `None`) of each of `ids`; floor: of everything on the floors `ids` (or
    /// the floors of the objects `ids`); plan: make `material` the default
    /// for `surface` in the active default set. `material: None` resets to
    /// the default (the default painter).
    PaintMaterial {
        material: Option<Id>,
        #[serde(default)]
        scope: PaintScope,
        #[serde(default)]
        ids: Vec<Id>,
        surface: Option<Surface>,
    },
    /// Make the plan's terrain with this outline (replacing any terrain).
    AddTerrain { floor: Option<Id>, points: Vec<Point2> },
    /// Add a piece of elevation data shaping the terrain.
    AddElevationData { floor: Option<Id>, shape: ElevationShape },
    /// Add survey points as elevation points: one point per line as
    /// "x, y, elevation" (or whitespace-separated) in `unit`, or a GPX
    /// track/waypoint file (`gpx: true`, metres, placed around the first
    /// point). A terrain is made around them when the plan has none.
    ImportTerrainPoints {
        floor: Option<Id>,
        text: String,
        #[serde(default)]
        gpx: bool,
        #[serde(default)]
        unit: Option<DisplayUnit>,
    },
    /// Add elevation data shapes (contours, points, break lines… e.g. read
    /// from a survey drawing) on `floor`.
    AddElevationShapes { floor: Option<Id>, shapes: Vec<ElevationShape> },
    /// Divide closed lot outline `lot` (a CAD shape) into `count` lots of
    /// equal area by lines at `angle` degrees (across the lot's long side
    /// when `None`), each lot labelled with its number and area.
    SubdivideLot { lot: Id, count: u32, angle: Option<f64> },
    /// Remove the terrain and all elevation data.
    ClearTerrain,
    /// Build (or rebuild) automatic framing on `floor` (every floor when
    /// unset): walls, floor and ceiling joists and roof rafters, only for
    /// `hosts` (walls, floors or roof planes) when given. Members placed or
    /// edited by hand are kept.
    BuildFraming {
        floor: Option<Id>,
        #[serde(default = "crate::framing::FramingParts::all")]
        parts: crate::framing::FramingParts,
        #[serde(default)]
        hosts: Vec<Id>,
    },
    /// Remove framing on `floor` (every floor when unset); hand-placed
    /// members stay unless `manual_too`.
    ClearFraming {
        floor: Option<Id>,
        #[serde(default)]
        manual_too: bool,
    },
    /// Place a framing member by hand; unset sizes are 2x stock of the
    /// default framing depth for its role.
    AddFramingMember {
        floor: Option<Id>,
        role: FramingRole,
        a: Point3,
        b: Point3,
        width: Option<Length>,
        depth: Option<Length>,
        #[serde(default)]
        rotation: f64,
        #[serde(default)]
        material: FramingMaterial,
        /// Pieces side by side (beams and posts).
        #[serde(default)]
        plies: Option<u32>,
    },
    /// Draw the truss through a chord or web member as a labelled CAD detail.
    TrussDetail { member: Id },
    /// Set which way joists run in the room around `at`.
    AddJoistDirection {
        floor: Option<Id>,
        at: Point2,
        angle: f64,
        #[serde(default)]
        ceiling: bool,
    },
    /// Add a deck (or with `platform`, a raised floor platform) with
    /// typical sizes; `height` is the top above the floor (default: 8"
    /// below it for decks, 12" above for platforms).
    AddDeck {
        floor: Option<Id>,
        points: Vec<Point2>,
        height: Option<Length>,
        #[serde(default)]
        platform: bool,
        #[serde(default)]
        curved: bool,
    },
    /// Pin a note or dimension in 3D (to one camera's view, or every 3D view).
    AddAnnotation3d {
        #[serde(default)]
        camera: Option<Id>,
        mark: Mark3d,
    },
    /// Save a camera path.
    AddWalkthrough {
        name: String,
        keyframes: Vec<Keyframe>,
        fps: Option<f64>,
    },
    /// A walkthrough circling the whole model once in `seconds`.
    OrbitWalkthrough {
        name: String,
        #[serde(default = "orbit_seconds")]
        seconds: f64,
    },
    /// Draw a wall's framing (seen from outside) as a new CAD detail.
    WallFramingDetail { wall: Id },
    /// Draw a lot from a survey's bearings and distances, starting at
    /// `start`, each side labelled; a `setback` draws the building line
    /// inside a closed lot.
    AddPlotLines {
        floor: Option<Id>,
        start: Point2,
        legs: Vec<PlotLeg>,
        #[serde(default)]
        setback: Option<Length>,
    },
    /// Make a CAD detail of the site: the terrain outline, building
    /// footprints (filled), site features and plants as plain lines.
    SitePlanDetail { name: String },
    /// Add a retaining wall, road, fence, garden bed, stepping stones or
    /// stream along `points`.
    AddSiteFeature {
        floor: Option<Id>,
        points: Vec<Point2>,
        feature: SiteKind,
    },
    /// Plant a tree, shrub or grass; unset sizes are typical for its form.
    AddPlant {
        floor: Option<Id>,
        at: Point2,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        form: PlantForm,
        height: Option<Length>,
        spread: Option<Length>,
    },
    /// Add a 3D solid; an unset colour comes from the solid defaults.
    AddSolid {
        floor: Option<Id>,
        shape: SolidShape,
        #[serde(default)]
        elevation: Length,
        color: Option<Color>,
    },
    /// Join, intersect or cut closed CAD shapes (the first is cut by the
    /// rest); closed polylines replace them.
    CadBoolean { ids: Vec<Id>, operation: ShapeOp },
    /// Turn closed CAD shapes on a floor plan into solids pushed up by
    /// `height` (the solid default when unset); they replace the shapes.
    ExtrudeCad { ids: Vec<Id>, height: Option<Length> },
    /// Turn a CAD polyline into a solid turned about a vertical axis through
    /// its leftmost point: each point's x is a radius and y a height.
    RevolveCad { id: Id, sweep: Option<f64> },
    /// Add a patch of `material` on a floor (any outline) or on a wall face.
    AddMaterialRegion {
        floor: Option<Id>,
        material: Id,
        target: RegionTarget,
    },
    /// Save the active default set's surface materials and cabinet door
    /// style and hardware as a style palette.
    SaveStyle { name: String },
    /// Apply a style palette: its materials become the defaults; with
    /// `cabinets`, every cabinet takes its door style and hardware.
    ApplyStyle {
        style: Id,
        #[serde(default)]
        cabinets: bool,
    },
    /// Copy a material and paint the copy on `ids` (to change it for just
    /// those objects).
    CopyMaterialFor { material: Id, ids: Vec<Id> },
    /// Make everything that uses material `from` use `to` instead; with
    /// `delete`, `from` is removed (merging the two).
    ReplaceMaterial {
        from: Id,
        to: Id,
        #[serde(default)]
        delete: bool,
    },
    /// Add a CAD detail: a blank, named drafting sheet for construction
    /// details. `shapes` and `texts` fill it (e.g. linework converted from a
    /// plan or elevation by `housecraft-draft`'s detail-from-view).
    AddCadDetail {
        name: String,
        #[serde(default)]
        shapes: Vec<DetailShape>,
        #[serde(default)]
        texts: Vec<DetailText>,
    },
    /// Make a CAD block. With `ids`, those CAD shapes (all on one sheet) move
    /// into the block, relative to the insertion point `base` (default: their
    /// bounds' bottom-left corner), and a copy of the block is placed where
    /// they were. With `shapes`, the block is made from shapes given relative
    /// to its insertion point (e.g. from a library file).
    AddBlock {
        name: String,
        #[serde(default)]
        ids: Vec<Id>,
        base: Option<Point2>,
        #[serde(default)]
        shapes: Vec<DetailShape>,
    },
    /// Place a copy of a block with its insertion point at `at`.
    AddBlockRef {
        floor: Option<Id>,
        block: Id,
        at: Point2,
        #[serde(default)]
        rotation: f64,
        scale: Option<f64>,
        layer: Option<Id>,
    },
    /// Replace a placed block with separate copies of its CAD shapes.
    ExplodeBlock { id: Id },
    /// Choose the default set an elevation, section or detail view uses for
    /// drafting (dimension and text styles, layers); `None` uses the active one.
    SetViewDefaults {
        view_kind: ViewKind,
        default_set: Option<Id>,
    },
    /// Add a revision cloud around `points`; `label` defaults to the next
    /// revision number.
    AddRevisionCloud {
        floor: Option<Id>,
        points: Vec<Point2>,
        arc: Option<Length>,
        label: Option<String>,
    },
    /// Exchange everything drawn on two floors (walls, rooms, CAD, text...),
    /// keeping the floors' own heights.
    SwapFloors { a: Id, b: Id },
    /// Add a layered wall type.
    AddWallType {
        name: String,
        /// Layers listed exterior to interior.
        layers: Vec<WallLayer>,
    },
    /// Add a drawing layer.
    AddLayer {
        name: String,
        color: Option<Color>,
        /// Pen width on paper in mm (default 0.25).
        line_weight: Option<f64>,
        #[serde(default)]
        line_style: LineStyle,
    },
    /// Add a layer set, optionally copying another.
    AddLayerSet { name: String, copy_from: Option<Id> },
    /// Change how a layer looks in one layer set; omitted fields are left alone.
    SetLayerDisplay {
        layer_set: Id,
        layer: Id,
        visible: Option<bool>,
        color: Option<Color>,
        line_weight: Option<f64>,
        line_style: Option<LineStyle>,
    },
    /// Add a default set, copying `copy_from` or the active set.
    AddDefaultSet { name: String, copy_from: Option<Id> },
    /// Choose the default set used by commands.
    SetActiveDefaultSet { id: Id },
    /// Add a text style.
    AddTextStyle {
        name: String,
        height: Length,
        font: Option<String>,
        #[serde(default)]
        bold: bool,
        #[serde(default)]
        italic: bool,
        color: Option<Color>,
    },
    /// Add a room type.
    AddRoomType {
        name: String,
        ceiling_height: Option<Length>,
        #[serde(default)]
        floor_finish: String,
        #[serde(default = "yes")]
        living_area: bool,
    },
    /// Add one straight wall along a centreline. Wall-type layers run exterior
    /// to interior with the exterior on the left looking from start to end, so
    /// draw exterior walls clockwise around the building.
    AddWall {
        /// Defaults to the lowest floor.
        floor: Option<Id>,
        start: Point2,
        end: Point2,
        /// Defaults to the default set's exterior (or interior) wall type.
        wall_type: Option<Id>,
        /// Use the default interior wall type instead of the exterior one.
        #[serde(default)]
        interior: bool,
        /// Defaults to the floor's ceiling height.
        height: Option<Length>,
        /// A room divider (separates rooms, isn't built).
        #[serde(default)]
        invisible: bool,
    },
    /// Add a connected run of walls through `points`; `closed` adds the closing wall.
    AddWalls {
        floor: Option<Id>,
        points: Vec<Point2>,
        #[serde(default)]
        closed: bool,
        wall_type: Option<Id>,
        #[serde(default)]
        interior: bool,
        height: Option<Length>,
    },
    /// Move one or both wall endpoints. Walls joined at a moved endpoint follow
    /// unless `detach` is set.
    MoveWall {
        id: Id,
        start: Option<Point2>,
        end: Option<Point2>,
        #[serde(default)]
        detach: bool,
    },
    /// Set a wall's length, keeping `anchor` fixed. Joined walls follow.
    SetWallLength {
        id: Id,
        length: Length,
        #[serde(default)]
        anchor: LengthAnchor,
    },
    /// Add a door to a wall. Place it with `position` (distance from the wall
    /// start to the door centre) or `at` (a point projected onto the wall);
    /// default is the wall's midpoint.
    AddDoor {
        wall: Id,
        position: Option<Length>,
        at: Option<Point2>,
        style: Option<DoorStyle>,
        width: Option<Length>,
        height: Option<Length>,
        hinge: Option<Hinge>,
        /// Swing toward the wall's right (interior) side. Default true.
        swing_in: Option<bool>,
    },
    /// Add a window to a wall; placement as for `add_door`.
    AddWindow {
        wall: Id,
        position: Option<Length>,
        at: Option<Point2>,
        style: Option<WindowStyle>,
        width: Option<Length>,
        height: Option<Length>,
        /// Height of the window bottom above the floor; default from head height.
        sill: Option<Length>,
    },
    /// Add a CAD shape (line, polyline/box, arc, circle, ellipse, spline, point).
    AddCad {
        floor: Option<Id>,
        layer: Option<Id>,
        shape: Shape,
        color: Option<Color>,
        fill: Option<Color>,
        /// A patterned fill (hatch, crosshatch, dots, solid).
        #[serde(default)]
        fill_style: Option<FillStyle>,
    },
    /// Add a dimension.
    AddDimension {
        floor: Option<Id>,
        layer: Option<Id>,
        geometry: DimensionGeometry,
        text_override: Option<String>,
    },
    /// Add overall and per-jog dimensions around the outside of the building.
    AddExteriorDimensions {
        floor: Option<Id>,
        /// Which tiers to draw (kept for refreshes); the default set's when `None`.
        #[serde(default)]
        options: Option<AutoDims>,
    },
    /// Save (or with `null`, delete) a named set of automatic dimension tiers.
    SetAutoDimPreset { name: String, dims: Option<AutoDims> },
    /// Vertical dimensions in elevation or section `view`: each floor, its
    /// ceiling, and the ridge, beside the building.
    ElevationDimensions { view: Id },
    /// A story pole in elevation or section `view`: a marked staff of levels
    /// (floors, plates, ridge) with their heights.
    StoryPole { view: Id },
    /// Framing reference markers in elevation or section `view`: a datum
    /// triangle and height at each floor's joists, subfloor and top plate.
    FramingMarkers { view: Id },
    /// A column-grid line from `a` to `b` with a bubble at `a`; grid lines
    /// are numbered 1, 2, 3 left to right and lettered A, B, C bottom to top.
    AddGridLine { floor: Option<Id>, a: Point2, b: Point2 },
    /// Add a text note.
    AddText {
        floor: Option<Id>,
        layer: Option<Id>,
        position: Point2,
        text: String,
        style: Option<Id>,
        #[serde(default)]
        rotation: f64,
        #[serde(default)]
        justify: Justify,
        height: Option<Length>,
        /// Arrow tips for leaders from the text.
        #[serde(default)]
        leaders: Vec<Point2>,
    },
    /// Type an object's own label (replacing its kind's template from the
    /// default set; "-" hides it), or `None` to go back to the template.
    SetObjectLabel { id: Id, text: Option<String> },
    /// Outlets round every room on `floor` (default: the lowest storey): at
    /// most 12' apart along the walls, clear of doors; GFCI outlets in
    /// kitchens, baths, laundries and garages.
    AutoOutlets { floor: Option<Id> },
    /// Space planning: turn closed CAD boxes into rooms with walls, each
    /// room typed by the room-type name written inside its box.
    ConvertSpacePlan { ids: Vec<Id> },
    /// A dormer on the roof at plan point `at`, facing down the roof plane
    /// there unless `facing` is given.
    AddDormer {
        floor: Option<Id>,
        at: Point2,
        facing: Option<f64>,
        width: Option<Length>,
        wall_height: Option<Length>,
        pitch: Option<f64>,
        #[serde(default)]
        style: DormerStyle,
    },
    /// A skylight in the roof plane at plan point `at`.
    AddSkylight { at: Point2, width: Option<Length>, length: Option<Length> },
    /// Change a roof plane's pitch, pivoting on its eave (or on its ridge
    /// with `keep_ridge`).
    SetRoofPitch {
        id: Id,
        pitch: f64,
        #[serde(default)]
        keep_ridge: bool,
    },
    /// Two windows meeting at a corner: one at the end of `wall`, one at the
    /// start of the wall joined there (or at `wall`'s start and the wall
    /// ending there, with `at_start`).
    AddCornerWindow {
        wall: Id,
        #[serde(default)]
        at_start: bool,
        width: Length,
        height: Option<Length>,
        sill: Option<Length>,
        style: Option<WindowStyle>,
    },
    /// A bay, box or bow window: the wall is opened `width` wide around
    /// `position` and walls project `depth` outward, each with a window.
    AddBayWindow {
        wall: Id,
        position: Length,
        width: Length,
        depth: Length,
        #[serde(default)]
        bay: BayStyle,
        /// Bow windows: how many faces (default 5).
        #[serde(default)]
        faces: Option<u8>,
        style: Option<WindowStyle>,
    },
    /// Show angles as decimal degrees, DMS, bearings or azimuths.
    SetAngleFormat { format: housecraft_units::AngleFormat },
    /// Set an object's information fields (`None` clears them).
    SetObjectInfo { id: Id, info: Option<InfoFields> },
    /// Add or replace the named dimension style `name` (`defaults` None
    /// deletes it; dimensions using it fall back to their default set's).
    SetDimensionStyle { name: String, defaults: Option<DimensionDefaults> },
    /// Add the standard plan views — Electrical, Framing, Kitchen & Bath,
    /// Construction and Site — each with its own default set and layer set
    /// copied from the current ones (views that exist are left alone).
    AddStandardPlanViews,
    /// Add a line style to the plan (`index` None) or change one.
    SetLineType { index: Option<u32>, line_type: LineType },
    /// Delete one of the plan's line styles; whatever used it uses
    /// `merge_into` instead.
    DeleteLineType {
        index: u32,
        #[serde(default)]
        merge_into: LineStyle,
    },
    /// Bring in 2D shapes and notes (e.g. read from a DXF file) on `floor`,
    /// each on the layer of that name (made when missing; "0" or "" uses the
    /// CAD and Text layers), moved by `offset`.
    ImportDrawing {
        floor: Option<Id>,
        #[serde(default)]
        shapes: Vec<NamedShape>,
        #[serde(default)]
        texts: Vec<NamedText>,
        #[serde(default)]
        offset: Option<Point2>,
    },
    /// Trace a room's interior outline as a closed CAD polyline (on the CAD
    /// layer), e.g. for area takeoffs, slabs or custom moldings.
    RoomPolyline { room: Id },
    /// Save a plan view (floor, layer set, default set).
    AddPlanView {
        name: String,
        floor: Option<Id>,
        layer_set: Option<Id>,
        default_set: Option<Id>,
        zoom: Option<ViewRect>,
        #[serde(default)]
        reference_floor: bool,
        /// Reference floors to show (instead of the one below).
        #[serde(default)]
        references: Vec<Id>,
        #[serde(default)]
        phase: Phase,
    },
    /// Save a 3D camera.
    AddCamera {
        name: String,
        projection: Projection,
        eye: Point3,
        target: Point3,
        layer_set: Option<Id>,
        /// How the view is drawn (style, lines, sky, lights).
        #[serde(default)]
        render: Option<RenderSettings>,
    },
    /// Change any properties of any object with a JSON merge patch over its
    /// JSON form (see `get_object`); `null` clears an optional field.
    Update {
        id: Id,
        set: serde_json::Map<String, serde_json::Value>,
    },
    /// Move, rotate, mirror or scale objects. With `copies` > 0 the originals
    /// stay and that many copies are made, each transformed once more than the
    /// last (replicate at intervals). Openings alone slide along their wall.
    Transform {
        ids: Vec<Id>,
        transform: TransformOp,
        #[serde(default)]
        copies: u32,
    },
    /// Copy `from`'s properties (type, size, layer, materials, style…) onto
    /// each object in `to` of the same kind; where they are and which way
    /// they face is kept. `fields` limits it to those properties (as in
    /// [`crate::query::matchable_fields`]); empty copies them all.
    MatchProperties {
        from: Id,
        to: Vec<Id>,
        #[serde(default)]
        fields: Vec<String>,
    },
    /// Copy a CAD line/polyline/circle/arc or a wall parallel to itself;
    /// positive distance is to the left (outward for circles and arcs).
    Offset { id: Id, distance: Length },
    /// Trim or extend a CAD line or wall so its nearer end meets the line
    /// through `boundary` (a CAD line or wall).
    ExtendTo { id: Id, boundary: Id },
    /// Join two CAD lines (or two walls with radius 0) at their intersection,
    /// rounding the corner with an arc of `radius`.
    Fillet {
        a: Id,
        b: Id,
        #[serde(default)]
        radius: Length,
    },
    /// Cut the corner between two CAD lines with a straight segment.
    Chamfer { a: Id, b: Id, distance: Length },
    /// Insert copies of objects (as returned by `get_object`), e.g. from a
    /// clipboard. They get fresh ids; a door or window whose wall is in the
    /// same batch follows the new wall. Placed objects move to `floor` and
    /// shift by `dx`/`dy`.
    InsertObjects {
        objects: Vec<serde_json::Value>,
        floor: Option<Id>,
        #[serde(default)]
        dx: Length,
        #[serde(default)]
        dy: Length,
    },
    /// Delete objects. Floors take their contents with them; walls take their
    /// doors and windows. Objects still referenced elsewhere cannot be deleted.
    Delete { ids: Vec<Id> },
    /// Delete Objects: every object of the given kinds on `floor` (every
    /// floor when `None`), optionally only those inside `room`.
    DeleteObjects {
        floor: Option<Id>,
        kinds: Vec<ObjectKind>,
        #[serde(default)]
        room: Option<Id>,
    },
    /// Delete Dimensions on `floor` (every floor when `None`): automatic
    /// ones, manual ones, or both.
    DeleteDimensions {
        floor: Option<Id>,
        #[serde(default)]
        which: DimensionsToDelete,
    },
    /// Reverse Layers: flip a wall end for end so its layers face the other
    /// way, keeping doors, windows, niches and the top where they are.
    ReverseWallLayers { id: Id },
    /// Auto Room Dimension: a dimension along the inside of each wall of a
    /// room, `offset` in from the wall surface.
    AutoRoomDimension {
        room: Id,
        #[serde(default = "room_dim_offset")]
        offset: Length,
    },
}

/// Which dimensions Delete Dimensions removes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DimensionsToDelete {
    Automatic,
    Manual,
    #[default]
    All,
}

fn room_dim_offset() -> Length {
    Length::from_inches(12.0)
}

fn one() -> u32 {
    1
}

/// The shape of a bay window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BayStyle {
    /// Sides at 45°.
    #[default]
    Bay,
    /// Sides at 90°.
    Box,
    /// Faces on a curve.
    Bow,
}

/// A shape on a named layer, for [`Command::ImportDrawing`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NamedShape {
    #[serde(default)]
    pub layer: String,
    pub shape: Shape,
}

/// A note on a named layer, for [`Command::ImportDrawing`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NamedText {
    #[serde(default)]
    pub layer: String,
    pub position: Point2,
    pub text: String,
    pub height: Length,
    #[serde(default)]
    pub rotation: f64,
}

fn yes() -> bool {
    true
}

fn straight() -> StairShape {
    StairShape::Straight
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LengthAnchor {
    #[default]
    Start,
    End,
    Center,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransformOp {
    Translate {
        dx: Length,
        dy: Length,
    },
    /// Degrees counter-clockwise about `center`.
    Rotate {
        center: Point2,
        angle: f64,
    },
    /// Reflect across the line through `a` and `b`.
    Mirror {
        a: Point2,
        b: Point2,
    },
    Scale {
        center: Point2,
        factor: f64,
    },
}

/// What a command or batch changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Outcome {
    pub created: Vec<Id>,
    pub modified: Vec<Id>,
    pub deleted: Vec<Id>,
}

impl Outcome {
    pub(crate) fn merge(&mut self, other: Outcome) {
        self.created.extend(other.created);
        self.modified.extend(other.modified);
        self.deleted.extend(other.deleted);
    }

    /// Remove duplicates; objects created or deleted are not also "modified".
    pub(crate) fn dedup(&mut self) {
        for v in [&mut self.created, &mut self.modified, &mut self.deleted] {
            let mut seen = std::collections::BTreeSet::new();
            v.retain(|id| seen.insert(*id));
        }
        let created: std::collections::BTreeSet<Id> = self.created.iter().copied().collect();
        let deleted: std::collections::BTreeSet<Id> = self.deleted.iter().copied().collect();
        self.modified
            .retain(|id| !created.contains(id) && !deleted.contains(id));
        // Created then deleted within one transaction never existed.
        self.created.retain(|id| !deleted.contains(id));
        self.deleted.retain(|id| !created.contains(id));
    }
}

fn first_layer_set(doc: &Document, given: Option<Id>) -> Result<Id, ApiError> {
    match given {
        Some(s) => Ok(s),
        None => doc
            .iter::<LayerSet>()
            .next()
            .map(|s| s.id)
            .ok_or_else(|| ApiError::Invalid("the document has no layer sets".into())),
    }
}

impl Command {
    /// The `op` tag, for error messages.
    pub fn op(&self) -> &'static str {
        match self {
            Command::SetDocumentName { .. } => "set_document_name",
            Command::SetDisplayUnit { .. } => "set_display_unit",
            Command::AddFloor { .. } => "add_floor",
            Command::SwapFloors { .. } => "swap_floors",
            Command::BuildFoundation { .. } => "build_foundation",
            Command::BuildRoof { .. } => "build_roof",
            Command::SetRoofStyle { .. } => "set_roof_style",
            Command::RemoveRoof { .. } => "remove_roof",
            Command::AddRoofPlane { .. } => "add_roof_plane",
            Command::AddStair { .. } => "add_stair",
            Command::AddCallout { .. } => "add_callout",
            Command::AddRevisionCloud { .. } => "add_revision_cloud",
            Command::AddCadDetail { .. } => "add_cad_detail",
            Command::AddMaterial { .. } => "add_material",
            Command::AddMaterialRegion { .. } => "add_material_region",
            Command::AddSolid { .. } => "add_solid",
            Command::AddTerrain { .. } => "add_terrain",
            Command::AddElevationData { .. } => "add_elevation_data",
            Command::ImportTerrainPoints { .. } => "import_terrain_points",
            Command::AddElevationShapes { .. } => "add_elevation_shapes",
            Command::SubdivideLot { .. } => "subdivide_lot",
            Command::ClearTerrain => "clear_terrain",
            Command::BuildFraming { .. } => "build_framing",
            Command::ClearFraming { .. } => "clear_framing",
            Command::AddFramingMember { .. } => "add_framing_member",
            Command::AddJoistDirection { .. } => "add_joist_direction",
            Command::TrussDetail { .. } => "truss_detail",
            Command::WallFramingDetail { .. } => "wall_framing_detail",
            Command::AddDeck { .. } => "add_deck",
            Command::AddWalkthrough { .. } => "add_walkthrough",
            Command::AddAnnotation3d { .. } => "add_annotation3d",
            Command::OrbitWalkthrough { .. } => "orbit_walkthrough",
            Command::AddPlotLines { .. } => "add_plot_lines",
            Command::SitePlanDetail { .. } => "site_plan_detail",
            Command::AddSiteFeature { .. } => "add_site_feature",
            Command::AddPlant { .. } => "add_plant",
            Command::CadBoolean { .. } => "cad_boolean",
            Command::ExtrudeCad { .. } => "extrude_cad",
            Command::RevolveCad { .. } => "revolve_cad",
            Command::SaveStyle { .. } => "save_style",
            Command::ApplyStyle { .. } => "apply_style",
            Command::CopyMaterialFor { .. } => "copy_material_for",
            Command::AddCabinet { .. } => "add_cabinet",
            Command::AddCountertop { .. } => "add_countertop",
            Command::AddAppliance { .. } => "add_appliance",
            Command::AutoFillers { .. } => "auto_fillers",
            Command::AddBacksplash { .. } => "add_backsplash",
            Command::AddMoldingProfile { .. } => "add_molding_profile",
            Command::AddMoldingPolyline { .. } => "add_molding_polyline",
            Command::AutoCabinetDimensions { .. } => "auto_cabinet_dimensions",
            Command::PlaceObject { .. } => "place_object",
            Command::AddPicture { .. } => "add_picture",
            Command::AddSymbolDef { .. } => "add_symbol_def",
            Command::ConvertToSymbol { .. } => "convert_to_symbol",
            Command::MuntinsFromCad { .. } => "muntins_from_cad",
            Command::PlaceOpening { .. } => "place_opening",
            Command::AddSymbol { .. } => "add_symbol",
            Command::ReplaceWith { .. } => "replace_with",
            Command::PaintMaterial { .. } => "paint_material",
            Command::ReplaceMaterial { .. } => "replace_material",
            Command::AddBlock { .. } => "add_block",
            Command::AddBlockRef { .. } => "add_block_ref",
            Command::ExplodeBlock { .. } => "explode_block",
            Command::SetViewDefaults { .. } => "set_view_defaults",
            Command::AddLayout { .. } => "add_layout",
            Command::AddLayoutPage { .. } => "add_layout_page",
            Command::NumberPages { .. } => "number_pages",
            Command::ConvertBoxToLines { .. } => "convert_box_to_lines",
            Command::DeleteLayoutBoxes { .. } => "delete_layout_boxes",
            Command::SetProjectInfo { .. } => "set_project_info",
            Command::PinObjects { .. } => "pin_objects",
            Command::LogTime { .. } => "log_time",
            Command::MoveWallByDimension { .. } => "move_wall_by_dimension",
            Command::MoveByDimension { .. } => "move_by_dimension",
            Command::AutoStairwell { .. } => "auto_stairwell",
            Command::PointToPointMove { .. } => "point_to_point_move",
            Command::ReflectAbout { .. } => "reflect_about",
            Command::MakeParallel { .. } => "make_parallel",
            Command::BreakWall { .. } => "break_wall",
            Command::DeleteTime { .. } => "delete_time",
            Command::SetHatchPattern { .. } => "set_hatch_pattern",
            Command::DeleteHatchPattern { .. } => "delete_hatch_pattern",
            Command::ConvertCad { .. } => "convert_cad",
            Command::RoofPlanesToPolylines { .. } => "roof_planes_to_polylines",
            Command::Scatter { .. } => "scatter",
            Command::AutoEndPanels { .. } => "auto_end_panels",
            Command::BreakStair { .. } => "break_stair",
            Command::AutoPostAndBeam { .. } => "auto_post_and_beam",
            Command::SetMacro { .. } => "set_macro",
            Command::ReplaceText { .. } => "replace_text",
            Command::ReplaceFont { .. } => "replace_font",
            Command::AddRevision { .. } => "add_revision",
            Command::DeleteRevision { .. } => "delete_revision",
            Command::SetWatermark { .. } => "set_watermark",
            Command::SendToLayout { .. } => "send_to_layout",
            Command::AddSectionView { .. } => "add_section_view",
            Command::AutoElevations { .. } => "auto_elevations",
            Command::RoomElevations { .. } => "room_elevations",
            Command::SetMaterialPrice { .. } => "set_material_price",
            Command::SetMarkup { .. } => "set_markup",
            Command::ImportPrices { .. } => "import_prices",
            Command::SetComponents { .. } => "set_components",
            Command::AddSchedule { .. } => "add_schedule",
            Command::AddElectrical { .. } => "add_electrical",
            Command::ConnectSwitch { .. } => "connect_switch",
            Command::SetRenovation { .. } => "set_renovation",
            Command::SetNorth { .. } => "set_north",
            Command::SetSun { .. } => "set_sun",
            Command::AddWallType { .. } => "add_wall_type",
            Command::AddLayer { .. } => "add_layer",
            Command::AddLayerSet { .. } => "add_layer_set",
            Command::SetLayerDisplay { .. } => "set_layer_display",
            Command::AddDefaultSet { .. } => "add_default_set",
            Command::SetActiveDefaultSet { .. } => "set_active_default_set",
            Command::AddTextStyle { .. } => "add_text_style",
            Command::AddRoomType { .. } => "add_room_type",
            Command::AddWall { .. } => "add_wall",
            Command::AddWalls { .. } => "add_walls",
            Command::MoveWall { .. } => "move_wall",
            Command::SetWallLength { .. } => "set_wall_length",
            Command::AddDoor { .. } => "add_door",
            Command::AddWindow { .. } => "add_window",
            Command::AddCad { .. } => "add_cad",
            Command::AddDimension { .. } => "add_dimension",
            Command::AddExteriorDimensions { .. } => "add_exterior_dimensions",
            Command::SetAutoDimPreset { .. } => "set_auto_dim_preset",
            Command::ElevationDimensions { .. } => "elevation_dimensions",
            Command::StoryPole { .. } => "story_pole",
            Command::FramingMarkers { .. } => "framing_markers",
            Command::AddGridLine { .. } => "add_grid_line",
            Command::AddText { .. } => "add_text",
            Command::SetObjectLabel { .. } => "set_object_label",
            Command::RoomPolyline { .. } => "room_polyline",
            Command::ImportDrawing { .. } => "import_drawing",
            Command::SetLineType { .. } => "set_line_type",
            Command::AddStandardPlanViews => "add_standard_plan_views",
            Command::SetAngleFormat { .. } => "set_angle_format",
            Command::AddCornerWindow { .. } => "add_corner_window",
            Command::AddDormer { .. } => "add_dormer",
            Command::AutoOutlets { .. } => "auto_outlets",
            Command::ConvertSpacePlan { .. } => "convert_space_plan",
            Command::AddSkylight { .. } => "add_skylight",
            Command::SetRoofPitch { .. } => "set_roof_pitch",
            Command::AddBayWindow { .. } => "add_bay_window",
            Command::SetObjectInfo { .. } => "set_object_info",
            Command::SetDimensionStyle { .. } => "set_dimension_style",
            Command::DeleteLineType { .. } => "delete_line_type",
            Command::AddPlanView { .. } => "add_plan_view",
            Command::AddCamera { .. } => "add_camera",
            Command::Update { .. } => "update",
            Command::Transform { .. } => "transform",
            Command::MatchProperties { .. } => "match_properties",
            Command::Offset { .. } => "offset",
            Command::ExtendTo { .. } => "extend_to",
            Command::Fillet { .. } => "fillet",
            Command::Chamfer { .. } => "chamfer",
            Command::InsertObjects { .. } => "insert_objects",
            Command::Delete { .. } => "delete",
            Command::DeleteObjects { .. } => "delete_objects",
            Command::DeleteDimensions { .. } => "delete_dimensions",
            Command::ReverseWallLayers { .. } => "reverse_wall_layers",
            Command::AutoRoomDimension { .. } => "auto_room_dimension",
        }
    }

    /// Apply to `doc` in place. On error `doc` may be partially modified, so
    /// callers apply to a scratch copy (see [`crate::Session::apply`]).
    pub(crate) fn apply(&self, doc: &mut Document) -> Result<Outcome, ApiError> {
        let mut out = Outcome::default();
        let d = doc.active_defaults().clone();
        match self {
            Command::SetDocumentName { name } => doc.name = name.clone(),
            Command::SetDisplayUnit { unit } => {
                if let DisplayUnit::FeetInches { denominator } | DisplayUnit::Inches { denominator } =
                    unit
                    && *denominator == 0
                {
                    return invalid("denominator must be at least 1");
                }
                doc.settings.display_unit = *unit;
            }
            Command::AddFloor {
                name,
                elevation,
                ceiling_height,
                floor_structure,
                below,
            } => {
                let structure = floor_structure.unwrap_or_else(default_floor_structure);
                if !(structure.is_finite() && structure.mm() >= 0.0) {
                    return invalid("floor_structure must not be negative");
                }
                let floors: Vec<Floor> = doc.floors_by_elevation().into_iter().cloned().collect();
                let name = name
                    .clone()
                    .unwrap_or_else(|| format!("Floor {}", doc.count::<Floor>() + 1));
                let id = match below {
                    Some(b) => {
                        let anchor = expect::<Floor>(doc, *b)?.clone();
                        let ceiling_height = ceiling_height.unwrap_or(anchor.ceiling_height);
                        let elevation = elevation.unwrap_or(anchor.elevation);
                        // Everything from the anchor floor up moves up by the new storey.
                        let shift = ceiling_height + anchor.floor_structure;
                        for f in floors
                            .iter()
                            .filter(|f| f.elevation.mm() >= anchor.elevation.mm())
                        {
                            if let Some(fm) = doc.get_mut::<Floor>(f.id) {
                                fm.elevation += shift;
                                out.modified.push(f.id);
                            }
                        }
                        doc.insert(Floor {
                            id: Id(0),
                            name,
                            elevation,
                            ceiling_height,
                            floor_structure: structure,
                            foundation: None,
                            auto_roof: false,
                            joist_spacing: None,
                            rim_material: None,
                            platform: if floor_structure.is_none() {
                                housecraft_model::default_platform()
                            } else {
                                Vec::new()
                            },
                        })
                    }
                    None => {
                        let top = floors.last();
                        let ceiling_height = ceiling_height
                            .or(top.map(|f| f.ceiling_height))
                            .unwrap_or(Length::from_feet(8.0));
                        let elevation = elevation
                            .or(top.map(|f| f.elevation + f.ceiling_height + structure))
                            .unwrap_or(Length::ZERO);
                        doc.insert(Floor {
                            id: Id(0),
                            name,
                            elevation,
                            ceiling_height,
                            floor_structure: structure,
                            foundation: None,
                            auto_roof: false,
                            joist_spacing: None,
                            rim_material: None,
                            platform: if floor_structure.is_none() {
                                housecraft_model::default_platform()
                            } else {
                                Vec::new()
                            },
                        })
                    }
                };
                out.created.push(id);
            }
            Command::BuildFoundation {
                kind,
                wall_height,
                wall_thickness,
                footing_width,
                footing_depth,
                slab_thickness,
                auto_rebuild,
                pier_spacing,
                pier_diameter,
                square_piers,
                garage_curb,
            } => {
                let inch = Length::from_inches;
                let spec = Foundation {
                    kind: *kind,
                    wall_height: wall_height.unwrap_or(match kind {
                        FoundationKind::Slab => inch(4.0),
                        FoundationKind::Crawlspace => inch(30.0),
                        FoundationKind::Basement => inch(96.0),
                        FoundationKind::Piers => inch(24.0),
                    }),
                    wall_thickness: wall_thickness.unwrap_or(inch(8.0)),
                    footing_width: footing_width.unwrap_or(match kind {
                        FoundationKind::Slab => inch(12.0),
                        _ => inch(16.0),
                    }),
                    footing_depth: footing_depth.unwrap_or(match kind {
                        FoundationKind::Slab => inch(12.0),
                        _ => inch(8.0),
                    }),
                    slab_thickness: slab_thickness.unwrap_or(inch(4.0)),
                    auto_rebuild: *auto_rebuild,
                    pier_spacing: pier_spacing.unwrap_or(Length::from_feet(6.0)),
                    pier_diameter: pier_diameter.unwrap_or(inch(12.0)),
                    square_piers: *square_piers,
                    garage_curb: garage_curb.unwrap_or(Length::ZERO),
                };
                if spec.pier_spacing.mm() < Length::from_feet(2.0).mm() {
                    return invalid("pier_spacing must be at least 2'");
                }
                crate::foundation::build(doc, spec, &mut out)?;
            }
            Command::SetRoofStyle {
                floor,
                style,
                pitch,
                lower_pitch,
                break_height,
            } => {
                crate::roofs::set_style(
                    doc,
                    *floor,
                    *style,
                    *pitch,
                    *lower_pitch,
                    *break_height,
                    &mut out,
                )?;
            }
            Command::BuildRoof {
                floor,
                auto_rebuild,
            } => {
                crate::roofs::build(doc, *floor, *auto_rebuild, &mut out)?;
            }
            Command::RemoveRoof { floor } => crate::roofs::remove(doc, *floor, &mut out)?,
            Command::AddRoofPlane {
                floor,
                points,
                eave_a,
                eave_b,
                eave_height,
                pitch,
            } => crate::roofs::add_plane(
                doc,
                *floor,
                points,
                *eave_a,
                *eave_b,
                *eave_height,
                *pitch,
                &mut out,
            )?,
            Command::AddStair {
                floor,
                start,
                direction,
                shape,
                width,
                rise,
                railing_left,
                railing_right,
                stairwell,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if !direction.is_finite() {
                    return invalid("direction must be a number of degrees");
                }
                // Stairs starting outside the building use the exterior defaults.
                let inside = housecraft_geom::rooms::detect(doc, floor)
                    .footprints
                    .iter()
                    .any(|f| {
                        housecraft_geom::math::point_in_polygon(housecraft_geom::v2(*start), f)
                    });
                let sd = if inside { &d.stair } else { &d.exterior_stair };
                let mut stair = Stair {
                    id: Id(0),
                    floor,
                    start: *start,
                    direction: *direction,
                    width: width.unwrap_or(sd.width),
                    shape: *shape,
                    rise: *rise,
                    max_riser: sd.max_riser,
                    tread_depth: sd.tread_depth,
                    railing_left: railing_left.unwrap_or(sd.railing_left),
                    railing_right: railing_right.unwrap_or(sd.railing_right),
                    railing_height: sd.railing_height,
                    tread_overhang: sd.tread_overhang,
                    stringers: sd.stringers,
                    stairwell: *stairwell,
                    layer: d.layers.stairs.unwrap_or(d.layers.walls),
                    baluster: None,
                    newel: None,
                    extras: Default::default(),
                    sections: Vec::new(),
                };
                // Split L and U stairs evenly when asked to.
                let risers =
                    ((doc.stair_rise(&stair).mm() / stair.max_riser.mm()).ceil() as u32).max(1);
                let treads = housecraft_geom::stairs::tread_count(risers, &stair.shape);
                match &mut stair.shape {
                    StairShape::L { first_treads, .. } | StairShape::U { first_treads, .. }
                        if *first_treads == 0 =>
                    {
                        *first_treads = (treads / 2).max(1);
                    }
                    _ => {}
                }
                out.created.push(doc.insert(stair));
            }
            Command::AddElectrical {
                floor,
                electrical_kind,
                at,
                rotation,
                height,
                controls,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let mut position = *at;
                let mut angle = rotation.unwrap_or(0.0);
                let mut kind = *electrical_kind;
                let mut weatherproof = false;
                if (kind.on_wall() || kind.in_floor().is_some()) && rotation.is_none() {
                    match housecraft_geom::electrical::snap_to_wall(doc, floor, housecraft_geom::v2(*at), Length::from_feet(2.0).mm()) {
                        Some((q, a)) => {
                            position = housecraft_geom::pt(q);
                            angle = a;
                            // An outlet facing out of the building is weatherproof.
                            let front = q + housecraft_geom::DVec2::from_angle(a.to_radians()) * 150.0;
                            weatherproof = kind.is_outlet()
                                && !housecraft_geom::rooms::detect(doc, floor)
                                    .footprints
                                    .iter()
                                    .any(|f| housecraft_geom::math::point_in_polygon(front, f));
                        }
                        // An outlet out in the room goes in the floor.
                        None => kind = kind.in_floor().unwrap_or(kind),
                    }
                }
                if !controls.is_empty() && !electrical_kind.is_switch() {
                    return invalid("only switches control fixtures");
                }
                out.created.push(doc.insert(Electrical {
                    id: Id(0),
                    floor,
                    electrical_kind: kind,
                    position,
                    rotation: angle,
                    height: *height,
                    controls: controls.clone(),
                    renovation: Default::default(),
                    layer: d.layers.electrical.unwrap_or(d.layers.cad),
                    light: None,
                    fixture: None,
                    weatherproof,
                    landscape: false,
                }));
            }
            Command::ConnectSwitch {
                switch,
                fixtures,
                disconnect,
            } => {
                let sw = expect::<Electrical>(doc, *switch)?;
                if !sw.electrical_kind.is_switch() {
                    return invalid(format!(
                        "{switch} is a {}, not a switch",
                        sw.electrical_kind.name()
                    ));
                }
                for f in fixtures {
                    let fx = expect::<Electrical>(doc, *f)?;
                    if fx.electrical_kind.is_switch() {
                        return invalid(format!("{f} is a switch; connect switches to fixtures"));
                    }
                }
                let sw = doc.get_mut::<Electrical>(*switch).expect("checked");
                for f in fixtures {
                    if *disconnect {
                        sw.controls.retain(|c| c != f);
                    } else if !sw.controls.contains(f) {
                        sw.controls.push(*f);
                    }
                }
                out.modified.push(*switch);
            }
            Command::SetRenovation { ids, status } => {
                for id in ids {
                    let slot = match doc.object_mut(*id).ok_or(ApiError::NotFound(*id))? {
                        Object::Wall(w) => &mut w.renovation,
                        Object::Opening(o) => &mut o.renovation,
                        Object::Electrical(e) => &mut e.renovation,
                        other => {
                            return invalid(format!("a {} has no renovation status", other.kind()));
                        }
                    };
                    *slot = *status;
                    out.modified.push(*id);
                }
            }
            Command::SetNorth { angle, pointer } => {
                if !angle.is_finite() {
                    return invalid("north angle must be a number of degrees");
                }
                doc.settings.north = angle.rem_euclid(360.0);
                doc.settings.north_pointer = *pointer;
            }
            Command::SetSun {
                azimuth,
                altitude,
                latitude,
                day_of_year,
                hour,
                shadows,
            } => {
                let sun = &mut doc.settings.sun;
                match (latitude, day_of_year, hour) {
                    (Some(lat), Some(day), Some(h)) => {
                        if !(-90.0..=90.0).contains(lat)
                            || !(1..=366).contains(day)
                            || !(0.0..=24.0).contains(h)
                        {
                            return invalid("latitude -90..90, day_of_year 1..366, hour 0..24");
                        }
                        let (az, alt) = housecraft_geom::sun::position(*lat, *day, *h);
                        sun.azimuth = az;
                        sun.altitude = alt;
                    }
                    (None, None, None) => {}
                    _ => return invalid("give latitude, day_of_year and hour together"),
                }
                if let Some(a) = azimuth {
                    sun.azimuth = a.rem_euclid(360.0);
                }
                if let Some(a) = altitude {
                    if !(-90.0..=90.0).contains(a) {
                        return invalid("altitude must be between -90 and 90 degrees");
                    }
                    sun.altitude = *a;
                }
                if let Some(s) = shadows {
                    sun.shadows = *s;
                }
            }
            Command::AddSchedule {
                floor,
                schedule_kind,
                position,
                columns,
                all_floors,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let known = crate::schedule::available_columns(*schedule_kind);
                if let Some(bad) = columns
                    .iter()
                    .find(|c| !known.iter().any(|k| k.eq_ignore_ascii_case(c)))
                {
                    return invalid(format!(
                        "{} has no column {bad:?} (has: {})",
                        schedule_kind.name(),
                        known.join(", ")
                    ));
                }
                out.created.push(doc.insert(Schedule {
                    id: Id(0),
                    floor,
                    layer: d.layers.text,
                    position: *position,
                    schedule_kind: *schedule_kind,
                    columns: columns.clone(),
                    all_floors: *all_floors,
                    text_height: None,
                    also: Vec::new(),
                    renovation: None,
                    start_number: None,
                    thumbnails: false,
                }));
            }
            Command::SetMaterialPrice { item, price } => match price {
                Some(p) if !p.is_finite() || *p < 0.0 => {
                    return invalid("price must be a non-negative number");
                }
                Some(p) => {
                    doc.settings.prices.insert(item.clone(), *p);
                }
                None => {
                    doc.settings.prices.remove(item);
                }
            },
            Command::SetMarkup { percent } => {
                if !percent.is_finite() || *percent < 0.0 {
                    return invalid("markup must be a non-negative percentage");
                }
                doc.settings.markup_percent = *percent;
            }
            Command::AddSectionView {
                name,
                floor,
                a,
                b,
                section_kind,
                depth,
                clip_sides,
            } => {
                let floor = crate::views::callout_floor(doc, *floor)?;
                let n = doc.count::<SectionView>() + 1;
                let name = name.clone().unwrap_or_else(|| match section_kind {
                    SectionKind::Section => format!("Section {n}"),
                    SectionKind::Elevation => format!("Elevation {n}"),
                });
                out.created.push(doc.insert(SectionView {
                    id: Id(0),
                    name,
                    floor,
                    layer: d.layers.text,
                    a: *a,
                    b: *b,
                    section_kind: *section_kind,
                    depth: *depth,
                    clip_sides: *clip_sides,
                    crop_left: None,
            crop_right: None,
            crop_bottom: None,
            crop_top: None,
            ignore_above: false,
            steps: Vec::new(),
            hidden_below_grade: true,
            schedule_labels: true,
                }));
            }
            Command::AutoElevations { floor } => {
                crate::views::auto_elevations(doc, *floor, &d, &mut out)?;
            }
            Command::RoomElevations { room } => {
                crate::views::room_elevations(doc, *room, &d, &mut out)?;
            }
            Command::AddLayout {
                name,
                paper_width,
                paper_height,
                pages,
            } => {
                let pages = (*pages).max(1);
                out.created.push(doc.insert(Layout {
                    id: Id(0),
                    name: name.clone(),
                    paper_width: paper_width.unwrap_or(Length::from_inches(36.0)),
                    paper_height: paper_height.unwrap_or(Length::from_inches(24.0)),
                    pages: (1..=pages).map(|n| format!("A-{n}")).collect(),
                    title_block: true,
                    titles: Vec::new(),
                    templates: Vec::new(),
                }));
            }
            Command::ImportPrices { prices, overwrite } => {
                for (k, v) in prices {
                    if !(v.is_finite() && *v >= 0.0) {
                        return Err(ApiError::Invalid(format!("bad price for {k}")));
                    }
                    if *overwrite || !doc.settings.prices.contains_key(k) {
                        doc.settings.prices.insert(k.clone(), *v);
                    }
                }
            }
            Command::SetComponents { id, components } => {
                if doc.object(*id).is_none() {
                    return Err(ApiError::NotFound(*id));
                }
                match components {
                    Some(c) if c.replace || !c.items.is_empty() => {
                        if c.items.iter().any(|i| !(i.quantity.is_finite() && i.quantity >= 0.0) || i.item.trim().is_empty()) {
                            return Err(ApiError::Invalid("components need a name and a quantity of 0 or more".into()));
                        }
                        doc.settings.components.insert(*id, c.clone());
                    }
                    _ => {
                        doc.settings.components.remove(id);
                    }
                }
                out.modified.push(*id);
            }
            Command::NumberPages { layout, prefix, start } => {
                let l = doc.get_mut::<Layout>(*layout).ok_or(ApiError::NotFound(*layout))?;
                for (i, p) in l.pages.iter_mut().enumerate() {
                    *p = format!("{prefix}{}", *start as usize + i);
                }
                out.modified.push(*layout);
            }
            Command::DeleteLayoutBoxes { layout, page, kind } => {
                crate::ops::expect::<Layout>(doc, *layout)?;
                let doomed: Vec<Id> = doc
                    .iter::<LayoutBox>()
                    .filter(|b| b.layout == *layout && page.is_none_or(|p| b.page == p))
                    .filter(|b| kind.as_deref().is_none_or(|k| crate::layouts::source_kind(&b.source) == k))
                    .map(|b| b.id)
                    .collect();
                for id in doomed {
                    doc.remove(id);
                    out.deleted.push(id);
                }
            }
            Command::ConvertBoxToLines { id, scale, shapes, texts } => {
                let b = crate::ops::expect::<LayoutBox>(doc, *id)?.clone();
                let name = format!("{} (lines)", b.label.clone().unwrap_or_else(|| crate::layouts::source_name(doc, &b.source)));
                let made = Command::AddCadDetail { name, shapes: shapes.clone(), texts: texts.clone() }.apply(doc)?;
                let detail = made.created[0];
                out.merge(made);
                let bx = doc.get_mut::<LayoutBox>(*id).ok_or(ApiError::NotFound(*id))?;
                bx.source = LayoutSource::Detail { detail };
                if *scale > 0.0 {
                    bx.scale = *scale;
                }
                out.modified.push(*id);
            }
            Command::SetMacro { name, text } => {
                let name = name.trim();
                if name.is_empty() || name.contains(['{', '}']) || name.chars().any(char::is_whitespace) {
                    return Err(ApiError::Invalid("a macro name is one word without braces".into()));
                }
                match text {
                    Some(t) => {
                        doc.settings.macros.insert(name.to_string(), t.clone());
                    }
                    None => {
                        doc.settings.macros.remove(name);
                    }
                }
            }
            Command::ReplaceText { find, replace, match_case, ids } => {
                if find.is_empty() {
                    return Err(ApiError::Invalid("nothing to find".into()));
                }
                for id in crate::ops::replace_text(doc, find, replace, *match_case, ids) {
                    out.modified.push(id);
                }
            }
            Command::ReplaceFont { from, to } => {
                for o in doc.objects.values_mut() {
                    if let Object::TextStyle(t) = o
                        && t.font.eq_ignore_ascii_case(from)
                    {
                        t.font = to.clone();
                        out.modified.push(t.id);
                    }
                }
            }
            Command::SetHatchPattern { index, pattern } => {
                if pattern.name.trim().is_empty() || pattern.lines.is_empty() {
                    return invalid("a hatch pattern needs a name and at least one line");
                }
                if pattern.lines.iter().any(|l| !(l.spacing.is_finite() && l.spacing > 0.0) || l.dash.iter().any(|d| !(d.is_finite() && *d > 0.0))) {
                    return invalid("line spacing and dash lengths must be positive");
                }
                match index {
                    Some(i) => {
                        let slot = doc.settings.hatch_patterns.get_mut(*i as usize).ok_or_else(|| ApiError::Invalid(format!("no hatch pattern {i}")))?;
                        *slot = pattern.clone();
                    }
                    None => doc.settings.hatch_patterns.push(pattern.clone()),
                }
            }
            Command::DeleteHatchPattern { index } => {
                let i = *index as usize;
                if i >= doc.settings.hatch_patterns.len() {
                    return invalid(format!("no hatch pattern {index}"));
                }
                doc.settings.hatch_patterns.remove(i);
                let ids: Vec<Id> = doc.objects.keys().copied().collect();
                for id in ids {
                    let mut v = serde_json::to_value(doc.object(id).expect("listed")).expect("serializes");
                    if ops::repoint_user_patterns(&mut v, *index) {
                        let object: Object = serde_json::from_value(v).map_err(|e| ApiError::Invalid(e.to_string()))?;
                        doc.objects.insert(id, object);
                        out.modified.push(id);
                    }
                }
            }
            Command::ConvertCad { ids, to } => crate::convert::convert_cad(doc, ids, to, &mut out)?,
            Command::RoofPlanesToPolylines { ids } => crate::convert::roof_outlines(doc, ids, &mut out)?,
            Command::AutoEndPanels { floor } => crate::cabinets::auto_end_panels(doc, *floor, &mut out),
            Command::BreakStair { id } => {
                let st = crate::ops::expect::<Stair>(doc, *id)?.clone();
                let g = housecraft_geom::stairs::geometry(doc, &st);
                let flight = |n: u32| StairSection::Flight { treads: n, width: None };
                let sections = match st.shape {
                    StairShape::Straight => vec![flight(g.flights.first().copied().unwrap_or(0))],
                    StairShape::L { turn, .. } | StairShape::U { turn, .. } => {
                        let (u_turn, gap) = match st.shape {
                            StairShape::U { gap, .. } => (true, gap),
                            _ => (false, Length::ZERO),
                        };
                        let (a, b) = (g.flights.first().copied().unwrap_or(0), g.flights.get(1).copied().unwrap_or(0));
                        // Winders become a plain landing.
                        let b = if st.extras.winders { b + 2 } else { b };
                        vec![flight(a), StairSection::Landing { turn: Some(turn), u_turn, gap, depth: None, split: false }, flight(b)]
                    }
                    StairShape::Curved { .. } | StairShape::Sections => return invalid("only straight, L and U stairs break into sections"),
                };
                let s = doc.get_mut::<Stair>(*id).ok_or(ApiError::NotFound(*id))?;
                s.shape = StairShape::Sections;
                s.sections = sections;
                s.extras.winders = false;
                out.modified.push(*id);
            }
            Command::AutoPostAndBeam { room, max_span, post_spacing } => {
                crate::framing::post_and_beam(doc, *room, max_span.map_or(12.0 * 304.8, |l| l.mm()), post_spacing.map_or(8.0 * 304.8, |l| l.mm()), &mut out)?
            }
            Command::Scatter { source, region, count, seed, size_variation, rotate } => {
                crate::convert::scatter(doc, *source, *region, *count, *seed, *size_variation, *rotate, &mut out)?
            }
            Command::BreakWall { id, at } if doc.get::<FramingMember>(*id).is_some() => {
                let m = expect::<FramingMember>(doc, *id)?.clone();
                let v3 = |p: Point3| housecraft_geom::DVec3::new(p.0.mm(), p.1.mm(), p.2.mm());
                let (a, b) = (v3(m.a), v3(m.b));
                let (p, _) = housecraft_geom::math::closest_on_segment(housecraft_geom::v2(*at), a.truncate(), b.truncate());
                let ab = (b - a).truncate();
                if m.is_vertical() || ab.length() < 1.0 {
                    return invalid("break a joist, beam or rafter, not an upright member");
                }
                let t = (p - a.truncate()).dot(ab) / ab.length_squared();
                if t * ab.length() < 1.0 || (1.0 - t) * ab.length() < 1.0 {
                    return invalid("break the member away from its ends");
                }
                let q = a.lerp(b, t);
                let q = Point3(Length::from_mm(q.x), Length::from_mm(q.y), Length::from_mm(q.z));
                let second = doc.insert(FramingMember { id: Id(0), a: q, manual: true, ..m.clone() });
                if let Some(first) = doc.get_mut::<FramingMember>(m.id) {
                    first.b = q;
                    first.manual = true;
                }
                out.modified.push(m.id);
                out.created.push(second);
            }
            Command::BreakWall { id, at } => {
                let w = expect::<Wall>(doc, *id)?.clone();
                let (a, b) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
                let (p, t) = housecraft_geom::math::closest_on_segment(housecraft_geom::v2(*at), a, b);
                let split = (p - a).length();
                if split < 1.0 || (b - p).length() < 1.0 {
                    return invalid("break the wall away from its ends");
                }
                let openings: Vec<Opening> = doc.openings_in(w.id).cloned().collect();
                if openings.iter().any(|o| (o.position.mm() - split).abs() < o.width.mm() / 2.0) {
                    return invalid("a door or window is in the way of the break");
                }
                let _ = t;
                let pt = housecraft_geom::math::pt(p);
                let second = doc.insert(Wall { id: Id(0), start: pt, ..w.clone() });
                if let Some(first) = doc.get_mut::<Wall>(w.id) {
                    first.end = pt;
                }
                for o in openings.into_iter().filter(|o| o.position.mm() > split) {
                    if let Some(om) = doc.get_mut::<Opening>(o.id) {
                        om.wall = second;
                        om.position = Length::from_mm(o.position.mm() - split);
                    }
                    out.modified.push(o.id);
                }
                out.modified.push(w.id);
                out.created.push(second);
            }
            Command::MoveWallByDimension { dimension, segment, wall, length } => {
                let (a, b) = ops::dimension_segment(doc, *dimension, *segment)?;
                let w = expect::<Wall>(doc, *wall)?.clone();
                let t = doc.get::<WallType>(w.wall_type).map_or(100.0, |t| t.thickness().mm());
                let axis = (b - a).normalize_or_zero();
                let (ws, we) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
                if axis == housecraft_geom::DVec2::ZERO || (we - ws).normalize_or_zero().dot(axis).abs() > 0.1 {
                    return invalid("the wall does not cross this dimension");
                }
                // The end of the segment on the wall (within its thickness).
                let on_wall = |p: housecraft_geom::DVec2| ((p - ws).dot(axis)).abs() <= t / 2.0 + 1.0;
                let (moving, fixed) = match (on_wall(a), on_wall(b)) {
                    (true, false) => (a, b),
                    (false, true) => (b, a),
                    _ => return invalid("neither end of that dimension segment is on the wall"),
                };
                let now = (moving - fixed).dot(axis).abs();
                let sign = (moving - fixed).dot(axis).signum();
                let shift = axis * sign * (length.mm() - now);
                let pt = housecraft_geom::math::pt;
                ops::move_wall_ends(doc, &w, pt(ws + shift), pt(we + shift), true, &mut out);
            }
            Command::MoveByDimension { dimension, segment, moving, ids, length } => {
                if ids.is_empty() {
                    return invalid("select what to move");
                }
                if *moving != *segment && *moving != segment + 1 {
                    return invalid("the moving point must be an end of the segment");
                }
                let (a, b) = ops::dimension_segment(doc, *dimension, *segment)?;
                let (m, f) = if *moving == *segment { (a, b) } else { (b, a) };
                let axis = (b - a).normalize_or_zero();
                if axis == housecraft_geom::DVec2::ZERO {
                    return invalid("that dimension segment has no length");
                }
                let now = (m - f).dot(axis).abs();
                let sign = (m - f).dot(axis).signum();
                let shift = axis * sign * (length.mm() - now);
                let pt = housecraft_geom::math::pt;
                match (ids.as_slice(), ids.first().and_then(|id| doc.get::<Wall>(*id)).cloned()) {
                    ([_], Some(w)) => {
                        let (ws, we) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
                        ops::move_wall_ends(doc, &w, pt(ws + shift), pt(we + shift), true, &mut out);
                    }
                    _ => {
                        let op = TransformOp::Translate { dx: Length::from_mm(shift.x), dy: Length::from_mm(shift.y) };
                        ops::transform(doc, ids, &op, 0, &mut out)?;
                    }
                }
                // A manual dimension keeps measuring to what moved.
                if !ids.contains(dimension)
                    && let Some(d) = doc.get_mut::<Dimension>(*dimension)
                    && !d.auto
                    && let DimensionGeometry::Linear { points, .. } = &mut d.geometry
                    && let Some(p) = points.get_mut(*moving)
                {
                    *p = pt(housecraft_geom::v2(*p) + shift);
                    out.modified.push(*dimension);
                }
            }
            Command::AutoStairwell { stair } => {
                let st = ops::expect::<Stair>(doc, *stair)?.clone();
                let Some(above) = doc.floor_above(st.floor).map(|f| f.id) else {
                    return invalid("there is no floor above the stair");
                };
                let Some(railing) = doc.iter::<WallType>().find(|t| t.railing.is_some()).map(|t| t.id) else {
                    return invalid("the plan has no railing wall type");
                };
                // The opening's extent along and across the stair.
                let g = housecraft_geom::stairs::geometry(doc, &st);
                let u = housecraft_geom::DVec2::from_angle(st.direction.to_radians());
                let v = housecraft_geom::math::perp(u);
                let pts: Vec<_> = g.steps.iter().flat_map(|s| s.poly.iter().copied()).collect();
                if pts.is_empty() {
                    return invalid("the stair has no steps");
                }
                let span = |d: housecraft_geom::DVec2| pts.iter().map(|p| p.dot(d)).fold((f64::MAX, f64::MIN), |(lo, hi), x| (lo.min(x), hi.max(x)));
                let ((a0, a1), (b0, b1)) = (span(u), span(v));
                let at = |a: f64, b: f64| housecraft_geom::math::pt(u * a + v * b);
                if let Some(s) = doc.get_mut::<Stair>(*stair)
                    && !s.stairwell
                {
                    s.stairwell = true;
                    out.modified.push(*stair);
                }
                let walls = Command::AddWalls {
                    floor: Some(above),
                    points: vec![at(a1, b0), at(a0, b0), at(a0, b1), at(a1, b1)],
                    closed: false,
                    wall_type: Some(railing),
                    interior: true,
                    height: None,
                }
                .apply(doc)?;
                out.merge(walls);
            }
            Command::PointToPointMove { ids, from, to, copy } => {
                let d = housecraft_geom::v2(*to) - housecraft_geom::v2(*from);
                let op = TransformOp::Translate { dx: Length::from_mm(d.x), dy: Length::from_mm(d.y) };
                ops::transform(doc, ids, &op, u32::from(*copy), &mut out)?;
            }
            Command::ReflectAbout { ids, about, copy } => {
                if ids.contains(about) {
                    return invalid("reflect about something that isn't selected");
                }
                let (a, b) = ops::reflect_line(doc, *about)?;
                let pt = housecraft_geom::math::pt;
                ops::transform(doc, ids, &TransformOp::Mirror { a: pt(a), b: pt(b) }, u32::from(*copy), &mut out)?;
            }
            Command::MakeParallel { ids, edge, reference } => {
                let v2 = housecraft_geom::v2;
                let (e0, e1, r0, r1) = (v2(edge[0]), v2(edge[1]), v2(reference[0]), v2(reference[1]));
                let (de, dr) = (e1 - e0, r1 - r0);
                if de.length() < 1e-6 || dr.length() < 1e-6 {
                    return invalid("the edge and the reference line need a length");
                }
                let angle = ops::parallel_turn(de.to_angle().to_degrees(), dr.to_angle().to_degrees());
                if angle.abs() > 1e-9 {
                    let center = housecraft_geom::math::pt((e0 + e1) / 2.0);
                    ops::transform(doc, ids, &TransformOp::Rotate { center, angle }, 0, &mut out)?;
                }
            }
            Command::LogTime { task, minutes, note, date } => {
                if !(minutes.is_finite() && *minutes > 0.0) {
                    return invalid("minutes must be positive");
                }
                let entry = TimeEntry {
                    date: date.clone().unwrap_or_else(crate::layouts::today),
                    task: if task.trim().is_empty() { "General".into() } else { task.trim().into() },
                    minutes: *minutes,
                    note: note.clone(),
                };
                // More time on the same task the same day adds to its entry.
                match doc.settings.time_log.last_mut() {
                    Some(last) if last.date == entry.date && last.task == entry.task && last.note == entry.note => {
                        last.minutes += entry.minutes;
                    }
                    _ => doc.settings.time_log.push(entry),
                }
            }
            Command::DeleteTime { index } => match index {
                None => doc.settings.time_log.clear(),
                Some(i) if *i < doc.settings.time_log.len() => {
                    doc.settings.time_log.remove(*i);
                }
                Some(i) => return Err(ApiError::Invalid(format!("no time entry {i}"))),
            },
            Command::PinObjects { ids, pinned } => {
                for id in ids {
                    if doc.object(*id).is_none() {
                        return Err(ApiError::NotFound(*id));
                    }
                    doc.settings.pins.retain(|p| p != id);
                    if *pinned {
                        doc.settings.pins.push(*id);
                    }
                }
            }
            Command::SetProjectInfo { info } => {
                doc.settings.project = info.clone();
            }
            Command::AddRevision { description, by, date } => {
                let number = doc.settings.revisions.iter().map(|r| r.number).max().unwrap_or(0) + 1;
                doc.settings.revisions.push(Revision {
                    number,
                    date: date.clone().unwrap_or_else(crate::layouts::today),
                    description: description.clone(),
                    by: by.clone(),
                });
            }
            Command::DeleteRevision { number } => {
                let before = doc.settings.revisions.len();
                doc.settings.revisions.retain(|r| r.number != *number);
                if doc.settings.revisions.len() == before {
                    return Err(ApiError::Invalid(format!("no revision {number}")));
                }
            }
            Command::SetWatermark { watermark } => {
                if let Some(w) = watermark
                    && !(w.height.mm() > 0.0 && (0.0..=1.0).contains(&w.darkness))
                {
                    return Err(ApiError::Invalid("watermark height must be positive and darkness 0–1".into()));
                }
                doc.settings.watermark = watermark.clone();
            }
            Command::AddLayoutPage { layout, name } => {
                let l = doc
                    .get_mut::<Layout>(*layout)
                    .ok_or(ApiError::NotFound(*layout))?;
                let n = l.pages.len() + 1;
                l.pages
                    .push(name.clone().unwrap_or_else(|| format!("A-{n}")));
                out.modified.push(*layout);
            }
            Command::SendToLayout {
                layout,
                page,
                source,
                x,
                y,
                width,
                height,
                scale,
                label,
            } => {
                let b = crate::layouts::send(
                    doc,
                    *layout,
                    *page,
                    source.clone(),
                    [*x, *y, *width, *height],
                    *scale,
                    label.clone(),
                )?;
                out.created.push(b);
            }
            Command::AddCallout {
                floor,
                text,
                at,
                target,
                marker,
            } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                let layer = doc.defaults_for(floor).layers.text;
                out.created.push(doc.insert(Callout {
                    id: Id(0),
                    floor,
                    layer,
                    text: text.clone(),
                    at: *at,
                    target: *target,
                    marker: *marker,
                    height: None,
                    link: None,
                    note: false,
                }));
            }
            Command::AddRevisionCloud {
                floor,
                points,
                arc,
                label,
            } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                if points.len() < 3 {
                    return invalid("a revision cloud needs at least three points");
                }
                let next = doc.count::<RevisionCloud>() + 1;
                let layer = doc.defaults_for(floor).layers.text;
                out.created.push(doc.insert(RevisionCloud {
                    id: Id(0),
                    floor,
                    layer,
                    points: points.clone(),
                    arc: arc.unwrap_or(Length::from_inches(18.0)),
                    label: Some(label.clone().unwrap_or_else(|| next.to_string())),
                }));
            }
            Command::AddCabinet {
                floor,
                cabinet_kind,
                at,
                facing,
                width,
                depth,
                height,
                shape,
                sink,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(crate::cabinets::add_cabinet(
                    doc,
                    &d,
                    floor,
                    *cabinet_kind,
                    *at,
                    *facing,
                    [*width, *depth, *height],
                    *shape,
                    *sink,
                )?);
            }
            Command::AddCountertop {
                floor,
                points,
                height,
                thickness,
                edge,
                holes,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if points.len() < 3 {
                    return invalid("a countertop needs at least three points");
                }
                let c = &d.cabinets;
                let thickness = thickness.unwrap_or(c.countertop_thickness);
                out.created.push(doc.insert(Countertop {
                    id: Id(0),
                    floor,
                    layer: d.layers.cabinets.unwrap_or(d.layers.cad),
                    points: points.clone(),
                    height: height.unwrap_or(c.base_height + thickness),
                    thickness,
                    edge: edge.unwrap_or(c.edge),
                    holes: holes.clone(),
                }));
            }
            Command::AddAppliance {
                floor,
                appliance_kind,
                at,
                facing,
                width,
                depth,
                height,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(crate::cabinets::add_appliance(
                    doc,
                    &d,
                    floor,
                    *appliance_kind,
                    *at,
                    *facing,
                    [*width, *depth, *height],
                )?);
            }
            Command::AddPicture {
                floor,
                path,
                position,
                width,
                height,
                pixels,
                transparent,
            } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                if path.trim().is_empty() {
                    return invalid("choose an image file");
                }
                let height = match (height, pixels) {
                    (Some(h), _) => *h,
                    (None, Some([w, h])) if *w > 0 => *width * (*h as f64 / *w as f64),
                    _ => *width,
                };
                let layer = doc.defaults_for(floor).layers.cad;
                out.created.push(doc.insert(Picture {
                    id: Id(0),
                    floor,
                    layer,
                    path: path.clone(),
                    position: *position,
                    width: *width,
                    height,
                    rotation: 0.0,
                    transparent: *transparent,
                    opacity: 1.0,
                }));
            }
            Command::AddSymbolDef {
                name,
                positions,
                triangles,
                groups,
                behavior,
            } => {
                if triangles.is_empty() {
                    return invalid("a 3D symbol needs at least one triangle");
                }
                out.created.push(doc.insert(SymbolDef {
                    id: Id(0),
                    name: name.clone(),
                    positions: positions.clone(),
                    triangles: triangles.clone(),
                    groups: groups.clone(),
                    behavior: behavior.clone(),
                }));
            }
            Command::ConvertToSymbol { ids, name } => crate::symbols::convert(doc, ids, name, &mut out)?,
            Command::PlaceOpening { wall, at, template } => {
                let w = expect::<Wall>(doc, *wall)?.clone();
                let mut v = template.clone();
                let m = v
                    .as_object_mut()
                    .ok_or_else(|| ApiError::Invalid("the template must be an object".into()))?;
                let kind: OpeningKind = serde_json::from_value(m.get("opening_kind").cloned().unwrap_or_default())
                    .map_err(|e| ApiError::Invalid(format!("template: {e}")))?;
                let width: Length = m
                    .get("width")
                    .and_then(|x| serde_json::from_value(x.clone()).ok())
                    .unwrap_or(if kind.is_door() { d.door.width } else { d.window.width });
                let position = ops::opening_position(&w, None, Some(*at), width)?;
                m.insert("id".into(), serde_json::json!(0));
                m.insert("wall".into(), serde_json::json!(wall));
                m.insert("position".into(), serde_json::json!(position));
                m.insert(
                    "layer".into(),
                    serde_json::json!(if kind.is_door() { d.layers.doors } else { d.layers.windows }),
                );
                let o: Opening =
                    serde_json::from_value(v).map_err(|e| ApiError::Invalid(format!("template: {e}")))?;
                out.created.push(doc.insert(o));
            }
            Command::MuntinsFromCad { opening, ids } => {
                let o = expect::<Opening>(doc, *opening)?;
                if o.opening_kind.is_door() {
                    return invalid("muntins go on windows");
                }
                let mut segs = Vec::new();
                for id in ids {
                    let cad = expect::<Cad>(doc, *id)?;
                    for (pts, closed) in housecraft_geom::math::shape_polylines(&cad.shape) {
                        let n = pts.len();
                        let count = if closed { n } else { n.saturating_sub(1) };
                        segs.extend((0..count).map(|i| (pts[i], pts[(i + 1) % n])));
                    }
                }
                let all = segs.iter().flat_map(|(a, b)| [*a, *b]);
                let Some((lo, hi)) = housecraft_geom::math::bounds(all) else {
                    return invalid("choose CAD lines to make the grille from");
                };
                let size = (hi - lo).max(housecraft_geom::DVec2::splat(1e-6));
                let unit = |p: housecraft_geom::DVec2| {
                    let q = (p - lo) / size;
                    [q.x, q.y]
                };
                let custom = segs.iter().map(|(a, b)| [unit(*a), unit(*b)]).collect();
                let o = doc.get_mut::<Opening>(*opening).expect("checked");
                let mut m = o.details.muntins.clone().unwrap_or(Muntins {
                    across: 1,
                    up: 1,
                    ..Muntins::default()
                });
                m.custom = custom;
                o.details.muntins = Some(m);
                out.modified.push(*opening);
            }
            Command::AddSymbol {
                floor,
                def,
                at,
                facing,
                elevation,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let sd = expect::<SymbolDef>(doc, *def)?;
                let size = sd.size();
                if size.iter().any(|s| *s <= 0.0) {
                    return invalid("the symbol is flat; it needs width, depth and height");
                }
                let hung = match sd.behavior {
                    SymbolBehavior::WallHung { height } => Some(height),
                    _ => None,
                };
                let elevation = &elevation.or(hung);
                let template = serde_json::json!({
                    "def": def,
                    "width": Length::from_mm(size[0]),
                    "depth": Length::from_mm(size[1]),
                    "height": Length::from_mm(size[2]),
                    "elevation": elevation.unwrap_or(Length::ZERO),
                });
                out.created.push(crate::cabinets::place_object(
                    doc,
                    &d,
                    floor,
                    ObjectKind::Symbol,
                    &template,
                    *at,
                    *facing,
                )?);
            }
            Command::PlaceObject {
                floor,
                kind,
                template,
                at,
                facing,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(crate::cabinets::place_object(
                    doc, &d, floor, *kind, template, *at, *facing,
                )?);
            }
            Command::ReplaceWith {
                ids,
                template,
                all_identical,
            } => crate::cabinets::replace_with(doc, ids, template, *all_identical, &mut out)?,
            Command::AddBacksplash {
                floor,
                points,
                bottom,
                height,
                thickness,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if points.len() < 2 {
                    return invalid("a backsplash needs at least two points");
                }
                let c = &d.cabinets;
                out.created.push(doc.insert(Backsplash {
                    id: Id(0),
                    floor,
                    layer: d.layers.cabinets.unwrap_or(d.layers.cad),
                    points: points.clone(),
                    bottom: bottom.unwrap_or(c.base_height + c.countertop_thickness),
                    height: height.unwrap_or(Length::from_inches(18.0)),
                    thickness: thickness.unwrap_or(Length::from_inches(0.5)),
                }));
            }
            Command::AddMoldingProfile {
                name,
                points,
                from_cad,
            } => {
                let pts: Vec<[f64; 2]> = match from_cad {
                    Some(cad) => {
                        let shape = &expect::<Cad>(doc, *cad)?.shape;
                        let poly = housecraft_geom::math::shape_polylines(shape)
                            .into_iter()
                            .find(|(p, closed)| *closed && p.len() >= 3)
                            .ok_or_else(|| {
                                ApiError::Invalid("draw the profile as a closed polyline or box".into())
                            })?
                            .0;
                        // Its lower-left corner becomes the profile's origin.
                        let lo = poly.iter().copied().reduce(|a, b| a.min(b)).expect("points");
                        poly.iter().map(|p| [p.x - lo.x, p.y - lo.y]).collect()
                    }
                    None => points.clone(),
                };
                if pts.len() < 3 {
                    return invalid("a molding profile needs at least three points");
                }
                out.created.push(doc.insert(MoldingProfile {
                    id: Id(0),
                    name: name.clone(),
                    points: pts,
                }));
            }
            Command::AddMoldingPolyline {
                floor,
                points,
                closed,
                elevation,
                stack,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if points.len() < 2 || stack.is_empty() {
                    return invalid("a molding needs a path and at least one profile");
                }
                out.created.push(doc.insert(MoldingPolyline {
                    id: Id(0),
                    floor,
                    layer: d.layers.cabinets.unwrap_or(d.layers.cad),
                    points: points.clone(),
                    closed: *closed,
                    elevation: *elevation,
                    stack: stack.clone(),
                }));
            }
            Command::AutoCabinetDimensions { floor } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                crate::cabinets::auto_dimensions(doc, &d, floor, &mut out);
            }
            Command::AutoFillers { floor } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let max = d.cabinets.max_filler.mm();
                for (id, l, r) in housecraft_geom::cabinets::auto_fillers(doc, floor, max) {
                    let c = doc.get_mut::<Cabinet>(id).expect("listed");
                    c.filler_left = Length::from_mm(l);
                    c.filler_right = Length::from_mm(r);
                    out.modified.push(id);
                }
            }
            Command::AddMaterial {
                name,
                category,
                class,
                color,
                copy_from,
            } => {
                if name.trim().is_empty() {
                    return invalid("a material needs a name");
                }
                let mut m = match copy_from {
                    Some(src) => expect::<Material>(doc, *src)?.clone(),
                    None => Material::new(
                        name,
                        "",
                        class.unwrap_or_default(),
                        color.unwrap_or(Color::rgb(200, 200, 200)),
                    ),
                };
                if copy_from.is_some()
                    && let Some(c) = class
                {
                    let fresh = Material::new(name, "", *c, m.color);
                    (m.class, m.roughness, m.metallic, m.opacity, m.emissive) =
                        (*c, fresh.roughness, fresh.metallic, fresh.opacity, fresh.emissive);
                }
                m.name = name.clone();
                if let Some(c) = category {
                    m.category = c.clone();
                }
                if let Some(c) = color {
                    m.color = *c;
                }
                out.created.push(doc.insert(m));
            }
            Command::AddTerrain { floor, points } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if points.len() < 3 {
                    return invalid("a terrain outline needs at least three points");
                }
                let old: Vec<Id> = doc.iter::<Terrain>().map(|t| t.id).collect();
                let mut t = match old.first().and_then(|id| doc.get::<Terrain>(*id)) {
                    Some(t) => t.clone(),
                    None => Terrain::new(floor, d.layers.cad, Vec::new()),
                };
                if !old.is_empty() {
                    ops::delete(doc, &old, &mut out)?;
                }
                t.id = Id(0);
                t.floor = floor;
                t.perimeter = points.clone();
                out.created.push(doc.insert(t));
            }
            Command::AddElevationData { floor, shape } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(ElevationData {
                    id: Id(0),
                    floor,
                    layer: d.layers.cad,
                    shape: shape.clone(),
                }));
            }
            Command::ImportTerrainPoints { floor, text, gpx, unit } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let pts = crate::site::parse_points(text, *gpx, unit.unwrap_or(doc.settings.display_unit))?;
                if pts.is_empty() {
                    return invalid("no points found");
                }
                for (x, y, z) in &pts {
                    out.created.push(doc.insert(ElevationData {
                        id: Id(0),
                        floor,
                        layer: d.layers.cad,
                        shape: ElevationShape::Point {
                            at: Point2(Length::from_mm(*x), Length::from_mm(*y)),
                            elevation: Length::from_mm(*z),
                        },
                    }));
                }
                if doc.count::<Terrain>() == 0 {
                    let pad = 3000.0;
                    let lo = pts.iter().fold((f64::MAX, f64::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
                    let hi = pts.iter().fold((f64::MIN, f64::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
                    let q = |x: f64, y: f64| Point2(Length::from_mm(x), Length::from_mm(y));
                    let perimeter = vec![
                        q(lo.0 - pad, lo.1 - pad),
                        q(hi.0 + pad, lo.1 - pad),
                        q(hi.0 + pad, hi.1 + pad),
                        q(lo.0 - pad, hi.1 + pad),
                    ];
                    out.created.push(doc.insert(Terrain::new(floor, d.layers.cad, perimeter)));
                }
            }
            Command::AddPlotLines { floor, start, legs, setback } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if legs.is_empty() {
                    return invalid("give at least one side of the lot");
                }
                let mut pts = vec![housecraft_geom::v2(*start)];
                for leg in legs {
                    let dir = crate::site::parse_bearing(&leg.bearing)?;
                    let next = *pts.last().expect("start") + dir * leg.distance.mm();
                    let a = *pts.last().expect("start");
                    // The bearing and distance along the side, reading upright.
                    let mut angle = dir.to_angle().to_degrees();
                    if !(-90.0..=90.0).contains(&angle) {
                        angle += 180.0;
                    }
                    let mid = (a + next) / 2.0 + housecraft_geom::math::perp(dir) * 200.0;
                    out.created.push(doc.insert(Text {
                        id: Id(0),
                        floor,
                        layer: d.layers.text,
                        position: housecraft_geom::pt(mid),
                        text: format!("{}  {}", leg.bearing.trim(), leg.distance.format(doc.settings.display_unit)),
                        style: None,
                        rotation: angle,
                        justify: Justify::Center,
                        height: None,
                        bold: None,
                        color: None,
                        leaders: Vec::new(),
                        border: false,
                        wrap: None,
                    }));
                    pts.push(next);
                }
                // A lot that comes back to its start is closed.
                let closed = pts.len() > 3 && pts[0].distance(*pts.last().expect("points")) < 10.0;
                if closed {
                    pts.pop();
                }
                out.created.push(doc.insert(Cad {
                    id: Id(0),
                    floor,
                    layer: d.layers.cad,
                    shape: Shape::Polyline {
                        points: pts.iter().map(|p| housecraft_geom::pt(*p)).collect(),
                        closed,
                    },
                    color: None,
                    fill: None,
                    fill_style: None,
                }));
                if let (Some(s), true) = (setback, closed) {
                    let mut poly = pts.clone();
                    if housecraft_geom::math::signed_area(&poly) < 0.0 {
                        poly.reverse();
                    }
                    let inner = housecraft_geom::math::inset_polygon(&poly, s.mm());
                    if inner.len() >= 3 {
                        out.created.push(doc.insert(Cad {
                            id: Id(0),
                            floor,
                            layer: d.layers.cad,
                            shape: Shape::Polyline {
                                points: inner.iter().map(|p| housecraft_geom::pt(*p)).collect(),
                                closed: true,
                            },
                            color: Some(Color::rgb(200, 60, 60)),
                            fill: None,
                            fill_style: None,
                        }));
                    }
                }
            }
            Command::BuildFraming { floor, parts, hosts } => crate::framing::build(doc, *floor, *parts, hosts, &mut out)?,
            Command::ClearFraming { floor, manual_too } => {
                let ids: Vec<Id> = doc
                    .iter::<FramingMember>()
                    .filter(|m| floor.is_none_or(|f| f == m.floor) && (*manual_too || !m.manual))
                    .map(|m| m.id)
                    .collect();
                if !ids.is_empty() {
                    ops::delete(doc, &ids, &mut out)?;
                }
            }
            Command::AddFramingMember { floor, role, a, b, width, depth, rotation, material, plies } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let f = &d.framing;
                let default_depth = match role {
                    FramingRole::FloorJoist | FramingRole::RimJoist | FramingRole::Beam | FramingRole::Header => f.floor_joist_depth,
                    FramingRole::CeilingJoist => f.ceiling_joist_depth,
                    FramingRole::Rafter | FramingRole::Ridge => f.rafter_depth,
                    FramingRole::Post => Length::from_inches(5.5),
                    _ => Length::from_inches(3.5),
                };
                let default_width = if *role == FramingRole::Post { Length::from_inches(5.5) } else { f.lumber_width };
                let layer = d.layers.framing.unwrap_or(d.layers.cad);
                out.created.push(doc.insert(FramingMember {
                    id: Id(0),
                    floor,
                    layer,
                    role: *role,
                    a: *a,
                    b: *b,
                    width: width.unwrap_or(default_width),
                    depth: depth.unwrap_or(default_depth),
                    rotation: *rotation,
                    material: *material,
                    host: None,
                    manual: true,
                    plies: plies.unwrap_or(1).max(1),
                    tail: TailCut::Square,
                }));
            }
            Command::AddJoistDirection { floor, at, angle, ceiling } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(JoistDirection {
                    id: Id(0),
                    floor,
                    layer: d.layers.framing.unwrap_or(d.layers.cad),
                    at: *at,
                    angle: *angle,
                    ceiling: *ceiling,
                }));
            }
            Command::TrussDetail { member } => {
                let (name, shapes, texts) = crate::framing::truss_detail(doc, *member)?;
                let more = Command::AddCadDetail { name, shapes, texts }.apply(doc)?;
                out.created.extend(more.created);
            }
            Command::AddAnnotation3d { camera, mark } => {
                if let Some(c) = camera {
                    expect::<Camera>(doc, *c)?;
                }
                out.created.push(doc.insert(Annotation3d { id: Id(0), camera: *camera, mark: mark.clone() }));
            }
            Command::AddWalkthrough { name, keyframes, fps } => {
                if keyframes.is_empty() {
                    return invalid("a walkthrough needs at least one keyframe");
                }
                out.created.push(doc.insert(Walkthrough {
                    id: Id(0),
                    name: name.clone(),
                    keyframes: keyframes.clone(),
                    fps: fps.unwrap_or(24.0),
                    render: None,
                }));
            }
            Command::OrbitWalkthrough { name, seconds } => {
                // Round the walls of every floor, from above and to one side.
                let pts: Vec<housecraft_geom::DVec2> = doc
                    .floors_by_elevation()
                    .iter()
                    .flat_map(|f| housecraft_geom::walls::wall_geoms(doc, f.id))
                    .flat_map(|g| [g.start, g.end])
                    .collect();
                let Some((lo, hi)) = housecraft_geom::math::bounds(pts) else {
                    return invalid("draw some walls first");
                };
                let top = doc
                    .floors_by_elevation()
                    .last()
                    .map_or(2438.4, |f| (f.elevation + f.ceiling_height).mm());
                let c = (lo + hi) / 2.0;
                let r = ((hi - lo).length() * 0.9).max(6000.0);
                let n = 12;
                let step = seconds.max(1.0) / n as f64;
                let keyframes = (0..=n)
                    .map(|i| {
                        let a = std::f64::consts::TAU * i as f64 / n as f64 - std::f64::consts::FRAC_PI_2;
                        let e = c + housecraft_geom::DVec2::from_angle(a) * r;
                        Keyframe {
                            eye: Point3::from_mm(e.x, e.y, top + r * 0.35),
                            target: Point3::from_mm(c.x, c.y, top * 0.5),
                            seconds: if i == 0 { 0.0 } else { step },
                            pause: 0.0,
                        }
                    })
                    .collect();
                out.created.push(doc.insert(Walkthrough {
                    id: Id(0),
                    name: name.clone(),
                    keyframes,
                    fps: 24.0,
                    render: None,
                }));
            }
            Command::AddDeck { floor, points, height, platform, curved } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                if points.len() < 3 {
                    return invalid("a deck needs at least three corners");
                }
                let layer = d.layers.framing.unwrap_or(d.layers.cad);
                let mut dk = if *platform {
                    Deck::platform(floor, layer, points.clone(), height.unwrap_or(Length::from_inches(12.0)))
                } else {
                    Deck::new(floor, layer, points.clone(), height.unwrap_or(Length::from_inches(-8.0)))
                };
                dk.layer = d.layers.cad;
                dk.curved = *curved;
                out.created.push(doc.insert(dk));
            }
            Command::WallFramingDetail { wall } => {
                let (name, shapes, texts) = crate::framing::wall_detail(doc, *wall)?;
                let more = Command::AddCadDetail { name, shapes, texts }.apply(doc)?;
                out.created.extend(more.created);
            }
            Command::SitePlanDetail { name } => {
                let (shapes, texts) = crate::site::site_plan(doc);
                let more = Command::AddCadDetail { name: name.clone(), shapes, texts }.apply(doc)?;
                out.created.extend(more.created);
            }
            Command::AddSiteFeature { floor, points, feature } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(SiteFeature {
                    id: Id(0),
                    floor,
                    layer: d.layers.cad,
                    points: points.clone(),
                    feature: feature.clone(),
                }));
            }
            Command::AddPlant { floor, at, name, form, height, spread } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let (h, s) = form.typical();
                out.created.push(doc.insert(Plant {
                    id: Id(0),
                    floor,
                    layer: d.layers.cad,
                    at: *at,
                    name: name.clone().unwrap_or_else(|| form.name().to_string()),
                    form: *form,
                    height: height.unwrap_or(h),
                    spread: spread.unwrap_or(s),
                    years_to_mature: 0.0,
                    age: 0.0,
                    color: Color::rgb(80, 130, 60),
                    image: None,
                }));
            }
            Command::SubdivideLot { lot, count, angle } => crate::site::subdivide(doc, *lot, *count, *angle, &mut out)?,
            Command::AddElevationShapes { floor, shapes } => {
                for shape in shapes {
                    let o = Command::AddElevationData { floor: *floor, shape: shape.clone() }.apply(doc)?;
                    out.merge(o);
                }
            }
            Command::ClearTerrain => {
                let ids: Vec<Id> = doc
                    .objects
                    .values()
                    .filter(|o| matches!(o.kind(), ObjectKind::Terrain | ObjectKind::ElevationData))
                    .map(|o| o.id())
                    .collect();
                if !ids.is_empty() {
                    ops::delete(doc, &ids, &mut out)?;
                }
            }
            Command::AddSolid {
                floor,
                shape,
                elevation,
                color,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                out.created.push(doc.insert(Solid {
                    id: Id(0),
                    floor,
                    layer: d.layers.cad,
                    shape: shape.clone(),
                    elevation: *elevation,
                    color: color.unwrap_or(d.solids.color),
                }));
            }
            Command::CadBoolean { ids, operation } => crate::solids::cad_boolean(doc, ids, *operation, &mut out)?,
            Command::ExtrudeCad { ids, height } => crate::solids::extrude(doc, &d, ids, *height, &mut out)?,
            Command::RevolveCad { id, sweep } => crate::solids::revolve(doc, &d, *id, *sweep, &mut out)?,
            Command::AddMaterialRegion {
                floor,
                material,
                target,
            } => {
                expect::<Material>(doc, *material)?;
                let floor = match target {
                    RegionTarget::Wall { wall, .. } => expect::<Wall>(doc, *wall)?.floor,
                    RegionTarget::Floor { .. } => ops::resolve_floor(doc, *floor)?,
                };
                out.created.push(doc.insert(MaterialRegion {
                    id: Id(0),
                    floor,
                    layer: d.layers.rooms,
                    material: *material,
                    target: target.clone(),
                }));
            }
            Command::SaveStyle { name } => {
                if name.trim().is_empty() {
                    return invalid("a style needs a name");
                }
                out.created.push(doc.insert(StylePalette {
                    id: Id(0),
                    name: name.clone(),
                    materials: d.materials.clone(),
                    door_style: Some(d.cabinets.door_style),
                    hardware: Some(d.cabinets.hardware),
                }));
            }
            Command::ApplyStyle { style, cabinets } => {
                let st = expect::<StylePalette>(doc, *style)?.clone();
                let set = doc.settings.active_default_set;
                if let Some(ds) = doc.get_mut::<DefaultSet>(set) {
                    ds.materials.extend(st.materials.iter().map(|(k, v)| (*k, *v)));
                    if let Some(s) = st.door_style {
                        ds.cabinets.door_style = s;
                    }
                    if let Some(h) = st.hardware {
                        ds.cabinets.hardware = h;
                    }
                    out.modified.push(set);
                }
                if *cabinets {
                    for o in doc.objects.values_mut() {
                        if let Object::Cabinet(c) = o {
                            if let Some(s) = st.door_style {
                                c.door_style = s;
                            }
                            if let Some(h) = st.hardware {
                                c.hardware = h;
                            }
                            out.modified.push(c.id);
                        }
                    }
                }
            }
            Command::CopyMaterialFor { material, ids } => {
                let mut m = expect::<Material>(doc, *material)?.clone();
                m.name = format!("{} (copy)", m.name);
                let copy = doc.insert(m);
                out.created.push(copy);
                crate::paint::paint(doc, Some(copy), PaintScope::Object, ids, None, &mut out)?;
            }
            Command::PaintMaterial {
                material,
                scope,
                ids,
                surface,
            } => crate::paint::paint(doc, *material, *scope, ids, *surface, &mut out)?,
            Command::ReplaceMaterial { from, to, delete } => {
                crate::paint::replace(doc, *from, *to, *delete, &mut out)?
            }
            Command::AddCadDetail {
                name,
                shapes,
                texts,
            } => {
                if name.trim().is_empty() {
                    return invalid("a CAD detail needs a name");
                }
                let detail = doc.insert(CadDetail {
                    id: Id(0),
                    name: name.clone(),
                });
                out.created.push(detail);
                let layers = doc.defaults_for(detail).layers.clone();
                for s in shapes {
                    doc.insert(Cad {
                        id: Id(0),
                        floor: detail,
                        layer: layers.cad,
                        shape: s.shape.clone(),
                        color: s.color,
                        fill: s.fill,
                        fill_style: None,
                    });
                }
                for t in texts {
                    doc.insert(Text {
                        id: Id(0),
                        floor: detail,
                        layer: layers.text,
                        position: t.position,
                        text: t.text.clone(),
                        style: None,
                        rotation: t.rotation,
                        justify: t.justify,
                        height: t.height,
                        bold: None,
                        color: None,
                        leaders: Vec::new(),
                        border: false,
                        wrap: None,
                    });
                }
            }
            Command::AddBlock {
                name,
                ids,
                base,
                shapes,
            } => crate::blocks::add_block(doc, &d, name, ids, *base, shapes, &mut out)?,
            Command::AddBlockRef {
                floor,
                block,
                at,
                rotation,
                scale,
                layer,
            } => {
                expect::<Block>(doc, *block)?;
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                let cad_layer = doc.defaults_for(floor).layers.cad;
                if scale.is_some_and(|k| !(k.is_finite() && k > 0.0)) {
                    return invalid("scale must be positive");
                }
                out.created.push(doc.insert(BlockRef {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(cad_layer),
                    block: *block,
                    at: *at,
                    rotation: *rotation,
                    scale: scale.unwrap_or(1.0),
                    mirrored: false,
                }));
            }
            Command::ExplodeBlock { id } => crate::blocks::explode(doc, *id, &mut out)?,
            Command::SetViewDefaults {
                view_kind,
                default_set,
            } => {
                if let Some(s) = default_set {
                    expect::<DefaultSet>(doc, *s)?;
                }
                doc.settings.view_defaults.set(*view_kind, *default_set);
            }
            Command::SwapFloors { a, b } => {
                expect::<Floor>(doc, *a)?;
                expect::<Floor>(doc, *b)?;
                ops::swap_floors(doc, *a, *b, &mut out);
            }
            Command::AddWallType { name, layers } => {
                out.created.push(doc.insert(WallType {
                    id: Id(0),
                    name: name.clone(),
                    layers: layers.clone(),
                    railing: None,
                    cladding: None,
                    corner_trim: None,
                }));
            }
            Command::AddLayer {
                name,
                color,
                line_weight,
                line_style,
            } => {
                out.created.push(doc.insert(Layer {
                    id: Id(0),
                    name: name.clone(),
                    color: *color,
                    line_weight: line_weight.unwrap_or(0.25),
                    line_style: *line_style,
                    text_style: None,
                    in_materials_list: true,
                    fill: None,
                }));
            }
            Command::AddLayerSet { name, copy_from } => {
                let layers = match copy_from {
                    Some(src) => expect::<LayerSet>(doc, *src)?.layers.clone(),
                    None => BTreeMap::new(),
                };
                out.created.push(doc.insert(LayerSet {
                    id: Id(0),
                    name: name.clone(),
                    layers,
                }));
            }
            Command::SetLayerDisplay {
                layer_set,
                layer,
                visible,
                color,
                line_weight,
                line_style,
            } => {
                expect::<Layer>(doc, *layer)?;
                expect::<LayerSet>(doc, *layer_set)?;
                let set = doc.get_mut::<LayerSet>(*layer_set).expect("checked");
                let entry = set.layers.entry(*layer).or_default();
                if let Some(v) = visible {
                    entry.visible = *v;
                }
                if color.is_some() {
                    entry.color = *color;
                }
                if line_weight.is_some() {
                    entry.line_weight = *line_weight;
                }
                if line_style.is_some() {
                    entry.line_style = *line_style;
                }
                out.modified.push(*layer_set);
            }
            Command::AddDefaultSet { name, copy_from } => {
                let mut set = match copy_from {
                    Some(src) => expect::<DefaultSet>(doc, *src)?.clone(),
                    None => d.clone(),
                };
                set.name = name.clone();
                out.created.push(doc.insert(set));
            }
            Command::SetActiveDefaultSet { id } => {
                expect::<DefaultSet>(doc, *id)?;
                doc.settings.active_default_set = *id;
            }
            Command::AddTextStyle {
                name,
                height,
                font,
                bold,
                italic,
                color,
            } => {
                out.created.push(doc.insert(TextStyle {
                    id: Id(0),
                    name: name.clone(),
                    height: *height,
                    font: font.clone().unwrap_or_else(|| "Sans".into()),
                    bold: *bold,
                    italic: *italic,
                    color: *color,
                }));
            }
            Command::AddRoomType {
                name,
                ceiling_height,
                floor_finish,
                living_area,
            } => {
                out.created.push(doc.insert(RoomType {
                    id: Id(0),
                    name: name.clone(),
                    ceiling_height: *ceiling_height,
                    floor_finish: floor_finish.clone(),
                    wall_finish: String::new(),
                    living_area: *living_area,
                    moldings: vec![RoomMolding::base()],
                    exterior: false,
                }));
            }
            Command::AddWall {
                floor,
                start,
                end,
                wall_type,
                interior,
                height,
                invisible,
            } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                let wt = ops::resolve_wall_type(doc, &d, *wall_type, *interior || *invisible)?;
                let id = ops::add_wall(doc, floor, *start, *end, wt, *height, d.layers.walls)?;
                if *invisible && let Some(w) = doc.get_mut::<Wall>(id) {
                    w.invisible = true;
                }
                out.created.push(id);
            }
            Command::AddWalls {
                floor,
                points,
                closed,
                wall_type,
                interior,
                height,
            } => {
                if points.len() < 2 {
                    return invalid("add_walls needs at least two points");
                }
                if *closed && points.len() < 3 {
                    return invalid("a closed run needs at least three points");
                }
                let floor = ops::resolve_floor(doc, *floor)?;
                let wt = ops::resolve_wall_type(doc, &d, *wall_type, *interior)?;
                let mut segments: Vec<_> = points.windows(2).map(|w| (w[0], w[1])).collect();
                if *closed {
                    segments.push((*points.last().expect("len >= 3"), points[0]));
                }
                for (a, b) in segments {
                    out.created.push(ops::add_wall(
                        doc,
                        floor,
                        a,
                        b,
                        wt,
                        *height,
                        d.layers.walls,
                    )?);
                }
            }
            Command::MoveWall {
                id,
                start,
                end,
                detach,
            } => {
                let w = expect::<Wall>(doc, *id)?.clone();
                ops::move_wall_ends(
                    doc,
                    &w,
                    start.unwrap_or(w.start),
                    end.unwrap_or(w.end),
                    !*detach,
                    &mut out,
                );
            }
            Command::SetWallLength { id, length, anchor } => {
                positive(*length, "length")?;
                let w = expect::<Wall>(doc, *id)?.clone();
                let (s, e) = (housecraft_geom::v2(w.start), housecraft_geom::v2(w.end));
                let dir = (e - s).normalize_or_zero();
                let l = length.mm();
                let (ns, ne) = match anchor {
                    LengthAnchor::Start => (s, s + dir * l),
                    LengthAnchor::End => (e - dir * l, e),
                    LengthAnchor::Center => {
                        let c = (s + e) / 2.0;
                        (c - dir * l / 2.0, c + dir * l / 2.0)
                    }
                };
                ops::move_wall_ends(
                    doc,
                    &w,
                    housecraft_geom::pt(ns),
                    housecraft_geom::pt(ne),
                    true,
                    &mut out,
                );
            }
            Command::AddDoor {
                wall,
                position,
                at,
                style,
                width,
                height,
                hinge,
                swing_in,
            } => {
                let w = expect::<Wall>(doc, *wall)?.clone();
                let style = style.unwrap_or(d.door.style);
                let (dw, dh) = d.door.size_for(style);
                let width = width.unwrap_or(dw);
                let height = height.unwrap_or(dh);
                let position = ops::opening_position(&w, *position, *at, width)?;
                out.created.push(doc.insert(Opening {
                    id: Id(0),
                    wall: *wall,
                    opening_kind: OpeningKind::Door(style),
                    position,
                    width,
                    height,
                    sill: Length::ZERO,
                    hinge: hinge.unwrap_or(d.door.hinge),
                    swing_in: swing_in.unwrap_or(true),
                    open_percent: 0.0,
                    plan_swing: 90.0,
                    renovation: Default::default(),
                    layer: d.layers.doors,
                    details: Default::default(),
                }));
            }
            Command::AddWindow {
                wall,
                position,
                at,
                style,
                width,
                height,
                sill,
            } => {
                let w = expect::<Wall>(doc, *wall)?.clone();
                let width = width.unwrap_or(d.window.width);
                let height = height.unwrap_or(d.window.height);
                let position = ops::opening_position(&w, *position, *at, width)?;
                let sill = sill.unwrap_or(d.window.head_height - height);
                out.created.push(doc.insert(Opening {
                    id: Id(0),
                    wall: *wall,
                    opening_kind: OpeningKind::Window(style.unwrap_or(d.window.style)),
                    position,
                    width,
                    height,
                    sill,
                    hinge: Hinge::Left,
                    swing_in: true,
                    open_percent: 0.0,
                    plan_swing: 90.0,
                    renovation: Default::default(),
                    layer: d.layers.windows,
                    details: Default::default(),
                }));
            }
            Command::AddCad {
                floor,
                layer,
                shape,
                color,
                fill,
                fill_style,
            } => {
                let floor = ops::resolve_sheet(doc, *floor, true)?;
                let cad_layer = doc.defaults_for(floor).layers.cad;
                out.created.push(doc.insert(Cad {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(cad_layer),
                    shape: shape.clone(),
                    color: *color,
                    fill: *fill,
                    fill_style: *fill_style,
                }));
            }
            Command::AddDimension {
                floor,
                layer,
                geometry,
                text_override,
            } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                let dim_layer = doc.defaults_for(floor).layers.dimensions;
                out.created.push(doc.insert(Dimension {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(dim_layer),
                    geometry: geometry.clone(),
                    text_override: text_override.clone(),
                    auto: false,
                    style: doc.defaults_for(floor).dimension_style.clone(),
                }));
            }
            Command::AddExteriorDimensions { floor, options } => {
                let floor = ops::resolve_floor(doc, *floor)?;
                match options {
                    Some(o) => doc.settings.auto_dims.insert(floor, *o),
                    None => doc.settings.auto_dims.remove(&floor),
                };
                ops::exterior_dimensions(doc, floor, &d, &mut out)?;
            }
            Command::SetAutoDimPreset { name, dims } => {
                let name = name.trim();
                if name.is_empty() {
                    return invalid("a preset needs a name");
                }
                doc.settings.auto_dim_presets.retain(|p| p.name != name);
                if let Some(dims) = dims {
                    doc.settings.auto_dim_presets.push(NamedAutoDims { name: name.to_string(), dims: *dims });
                }
            }
            Command::ElevationDimensions { view } => crate::grids::elevation_dimensions(doc, *view, &mut out)?,
            Command::StoryPole { view } => crate::grids::story_pole(doc, *view, &mut out)?,
            Command::FramingMarkers { view } => crate::grids::framing_markers(doc, *view, &mut out)?,
            Command::AddGridLine { floor, a, b } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                crate::grids::add_grid_line(doc, floor, *a, *b, &mut out)?;
            }
            Command::AddText {
                floor,
                layer,
                position,
                text,
                style,
                rotation,
                justify,
                height,
                leaders,
            } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                let text_layer = doc.defaults_for(floor).layers.text;
                out.created.push(doc.insert(Text {
                    id: Id(0),
                    floor,
                    layer: layer.unwrap_or(text_layer),
                    position: *position,
                    text: text.clone(),
                    style: *style,
                    rotation: *rotation,
                    justify: *justify,
                    height: *height,
                    bold: None,
                    color: None,
                    leaders: leaders.clone(),
                    border: false,
                    wrap: None,
                }));
            }
            Command::AddStandardPlanViews => ops::standard_plan_views(doc, &mut out)?,
            Command::SetAngleFormat { format } => doc.settings.angle_format = *format,
            Command::AutoOutlets { floor } => ops::auto_outlets(doc, *floor, &mut out)?,
            Command::ConvertSpacePlan { ids } => ops::convert_space_plan(doc, ids, &mut out)?,
            Command::AddDormer { floor, at, facing, width, wall_height, pitch, style } => {
                let p = housecraft_geom::v2(*at);
                // The roof plane under the point gives the floor and facing.
                let plane = doc
                    .iter::<RoofPlane>()
                    .filter(|r| !r.gable && floor.is_none_or(|f| r.floor == f))
                    .find(|r| {
                        let pts: Vec<housecraft_geom::DVec3> = r.outline.iter().map(|q| { let [x, y, z] = q.mm(); housecraft_geom::DVec3::new(x, y, z) }).collect();
                        housecraft_geom::roof_extras::plane_z(&pts, p).is_some()
                    })
                    .cloned()
                    .ok_or_else(|| ApiError::Invalid("a dormer goes on a sloped roof plane".into()))?;
                let pts: Vec<housecraft_geom::DVec3> = plane.outline.iter().map(|q| { let [x, y, z] = q.mm(); housecraft_geom::DVec3::new(x, y, z) }).collect();
                let face = facing.unwrap_or_else(|| housecraft_geom::roof_extras::downslope(&pts).to_angle().to_degrees());
                let layer = doc.defaults_for(plane.floor).layers.roofs.unwrap_or(plane.layer);
                out.created.push(doc.insert(Dormer {
                    id: Id(0),
                    floor: plane.floor,
                    layer,
                    position: *at,
                    facing: face,
                    width: width.unwrap_or(Length::from_feet(5.0)),
                    wall_height: wall_height.unwrap_or(Length::from_feet(3.0)),
                    pitch: pitch.unwrap_or(plane.pitch.clamp(15.0, 60.0)),
                    style: *style,
                    window: true,
                    overhang: Length::from_inches(8.0),
                    thickness: plane.thickness,
                }));
            }
            Command::AddSkylight { at, width, length } => {
                let p = housecraft_geom::v2(*at);
                let plane = doc
                    .iter::<RoofPlane>()
                    .filter(|r| !r.gable)
                    .find(|r| {
                        let pts: Vec<housecraft_geom::DVec3> = r.outline.iter().map(|q| { let [x, y, z] = q.mm(); housecraft_geom::DVec3::new(x, y, z) }).collect();
                        housecraft_geom::roof_extras::plane_z(&pts, p).is_some()
                    })
                    .map(|r| r.id)
                    .ok_or_else(|| ApiError::Invalid("a skylight goes in a sloped roof plane".into()))?;
                if let Some(Object::RoofPlane(r)) = doc.objects.get_mut(&plane) {
                    r.skylights.push(Skylight {
                        center: *at,
                        width: width.unwrap_or(Length::from_inches(22.5)),
                        length: length.unwrap_or(Length::from_inches(46.5)),
                    });
                }
                out.modified.push(plane);
            }
            Command::SetRoofPitch { id, pitch, keep_ridge } => ops::set_roof_pitch(doc, *id, *pitch, *keep_ridge, &mut out)?,
            Command::AddCornerWindow { wall, at_start, width, height, sill, style } => {
                let w = expect::<Wall>(doc, *wall)?.clone();
                let corner = if *at_start { w.start } else { w.end };
                let near = |p: Point2| p.distance(corner).mm() < 1.0;
                let other = doc
                    .walls_on(w.floor)
                    .find(|x| x.id != w.id && (if *at_start { near(x.end) } else { near(x.start) }))
                    .cloned()
                    .ok_or_else(|| ApiError::Invalid("no wall joins that corner".into()))?;
                let half_of = |x: &Wall| doc.get::<WallType>(x.wall_type).map_or(0.0, |t| t.thickness().mm() / 2.0);
                // Each window ends at the other wall's face, leaving the corner post.
                let wm = width.mm();
                let p1 = if *at_start { half_of(&other) + wm / 2.0 } else { w.length().mm() - half_of(&other) - wm / 2.0 };
                let p2 = if *at_start { other.length().mm() - half_of(&w) - wm / 2.0 } else { half_of(&w) + wm / 2.0 };
                for (host, pos) in [(w.id, p1), (other.id, p2)] {
                    let o = Command::AddWindow {
                        wall: host,
                        position: Some(Length::from_mm(pos)),
                        at: None,
                        style: *style,
                        width: Some(*width),
                        height: *height,
                        sill: *sill,
                    }
                    .apply(doc)?;
                    out.merge(o);
                }
            }
            Command::AddBayWindow { wall, position, width, depth, bay, faces, style } => {
                ops::bay_window(doc, *wall, *position, *width, *depth, *bay, faces.unwrap_or(5), *style, &mut out)?;
            }
            Command::SetObjectInfo { id, info } => {
                if doc.object(*id).is_none() {
                    return Err(ApiError::NotFound(*id));
                }
                match info {
                    Some(i) if !i.is_empty() => doc.settings.object_info.insert(*id, i.clone()),
                    _ => doc.settings.object_info.remove(id),
                };
                out.modified.push(*id);
            }
            Command::SetDimensionStyle { name, defaults } => {
                let name = name.trim();
                if name.is_empty() {
                    return invalid("a dimension style needs a name");
                }
                let styles = &mut doc.settings.dimension_styles;
                let at = styles.iter().position(|s| s.name == name);
                match (defaults, at) {
                    (Some(d), Some(i)) => styles[i].defaults = d.clone(),
                    (Some(d), None) => styles.push(DimensionStyle { name: name.to_string(), defaults: d.clone() }),
                    (None, Some(i)) => {
                        styles.remove(i);
                    }
                    (None, None) => return invalid(format!("no dimension style {name:?}")),
                }
            }
            Command::SetLineType { index, line_type } => {
                if line_type.pattern.iter().any(|v| !(v.is_finite() && *v >= 0.0)) || !line_type.pattern.len().is_multiple_of(2) {
                    return invalid("a dash pattern is pairs of non-negative lengths (on, off, …)");
                }
                match index {
                    Some(i) => {
                        let Some(t) = doc.settings.line_types.get_mut(*i as usize) else {
                            return invalid(format!("no line style {i}"));
                        };
                        *t = line_type.clone();
                    }
                    None => doc.settings.line_types.push(line_type.clone()),
                }
            }
            Command::DeleteLineType { index, merge_into } => {
                let i = *index as usize;
                if i >= doc.settings.line_types.len() {
                    return invalid(format!("no line style {index}"));
                }
                if *merge_into == LineStyle::Custom(*index) {
                    return invalid("can't merge a line style into itself");
                }
                doc.settings.line_types.remove(i);
                // Re-point every use: the deleted one → merge_into, later ones shift down.
                let target = match *merge_into {
                    LineStyle::Custom(j) if j > *index => LineStyle::Custom(j - 1),
                    other => other,
                };
                let target = serde_json::to_value(target).expect("serializes");
                let ids: Vec<Id> = doc.objects.keys().copied().collect();
                for id in ids {
                    let mut v = serde_json::to_value(doc.object(id).expect("listed")).expect("serializes");
                    if ops::repoint_line_styles(&mut v, *index, &target) {
                        let object: Object = serde_json::from_value(v).map_err(|e| ApiError::Invalid(e.to_string()))?;
                        doc.objects.insert(id, object);
                        out.modified.push(id);
                    }
                }
            }
            Command::ImportDrawing { floor, shapes, texts, offset } => {
                let floor = ops::resolve_sheet(doc, *floor, false)?;
                let layers = doc.defaults_for(floor).layers.clone();
                let layer_for = |doc: &mut Document, out: &mut Outcome, name: &str, fallback: Id| -> Id {
                    let name = name.trim();
                    if name.is_empty() || name == "0" {
                        return fallback;
                    }
                    if let Some(l) = doc.iter::<Layer>().find(|l| l.name.eq_ignore_ascii_case(name)) {
                        return l.id;
                    }
                    let id = doc.insert(Layer {
                        id: Id(0),
                        name: name.to_string(),
                        color: None,
                        line_weight: 0.25,
                        line_style: LineStyle::default(),
                        text_style: None,
                        in_materials_list: false,
                        fill: None,
                    });
                    out.created.push(id);
                    id
                };
                let shift = offset.map(housecraft_geom::v2);
                let t = shift.map(housecraft_geom::transform::Transform2::translate);
                for s in shapes {
                    let layer = layer_for(doc, &mut out, &s.layer, layers.cad);
                    let shape = match &t {
                        Some(t) => t.apply_shape(&s.shape),
                        None => s.shape.clone(),
                    };
                    out.created.push(doc.insert(Cad { id: Id(0), floor, layer, shape, color: None, fill: None, fill_style: None }));
                }
                for x in texts {
                    let layer = layer_for(doc, &mut out, &x.layer, layers.text);
                    let position = match &t {
                        Some(t) => t.apply_point(x.position),
                        None => x.position,
                    };
                    out.created.push(doc.insert(Text {
                        id: Id(0),
                        floor,
                        layer,
                        position,
                        text: x.text.clone(),
                        style: None,
                        rotation: x.rotation,
                        justify: Justify::Left,
                        height: Some(x.height),
                        bold: None,
                        color: None,
                        leaders: Vec::new(),
                        border: false,
                        wrap: None,
                    }));
                }
            }
            Command::RoomPolyline { room } => {
                let r = expect::<Room>(doc, *room)?.clone();
                let regions = housecraft_geom::rooms::detect(doc, r.floor);
                let Some(i) = regions.room_at(housecraft_geom::v2(r.anchor)) else {
                    return invalid("that room is not enclosed by walls");
                };
                let points = regions.rooms[i].polygon.iter().map(|p| housecraft_geom::pt(*p)).collect();
                out.created.push(doc.insert(Cad {
                    id: Id(0),
                    floor: r.floor,
                    layer: doc.defaults_for(r.floor).layers.cad,
                    shape: Shape::Polyline { points, closed: true },
                    color: None,
                    fill: None,
                    fill_style: None,
                }));
            }
            Command::SetObjectLabel { id, text } => {
                if doc.object(*id).is_none() {
                    return Err(ApiError::NotFound(*id));
                }
                match text {
                    Some(t) => doc.settings.labels.insert(*id, t.clone()),
                    None => doc.settings.labels.remove(id),
                };
                out.modified.push(*id);
            }
            Command::AddPlanView {
                name,
                floor,
                layer_set,
                default_set,
                zoom,
                reference_floor,
                references,
                phase,
            } => {
                let layer_set = first_layer_set(doc, *layer_set)?;
                out.created.push(doc.insert(PlanView {
                    id: Id(0),
                    name: name.clone(),
                    floor: *floor,
                    layer_set,
                    default_set: default_set.unwrap_or(d.id),
                    zoom: zoom.clone(),
                    reference_floor: *reference_floor,
                    references: references.clone(),
                    phase: *phase,
                }));
            }
            Command::AddCamera {
                name,
                projection,
                eye,
                target,
                layer_set,
                render,
            } => {
                let layer_set = first_layer_set(doc, *layer_set)?;
                out.created.push(doc.insert(Camera {
                    id: Id(0),
                    name: name.clone(),
                    projection: projection.clone(),
                    eye: *eye,
                    target: *target,
                    layer_set,
                    backdrop: None,
                    render: render.clone().unwrap_or_default(),
                }));
            }
            Command::Update { id, set } => ops::update(doc, *id, set, &mut out)?,
            Command::Transform {
                ids,
                transform,
                copies,
            } => ops::transform(doc, ids, transform, *copies, &mut out)?,
            Command::MatchProperties { from, to, fields } => ops::match_properties(doc, *from, to, fields, &mut out)?,
            Command::Offset { id, distance } => ops::offset(doc, *id, *distance, &mut out)?,
            Command::ExtendTo { id, boundary } => ops::extend_to(doc, *id, *boundary, &mut out)?,
            Command::Fillet { a, b, radius } => ops::fillet(doc, *a, *b, *radius, &mut out)?,
            Command::Chamfer { a, b, distance } => {
                positive(*distance, "distance")?;
                ops::chamfer(doc, *a, *b, *distance, &mut out)?
            }
            Command::InsertObjects {
                objects,
                floor,
                dx,
                dy,
            } => ops::insert_objects(doc, objects, *floor, *dx, *dy, &mut out)?,
            Command::Delete { ids } => ops::delete(doc, ids, &mut out)?,
            Command::DeleteObjects { floor, kinds, room } => {
                let inside = match room {
                    Some(r) => {
                        let rm = expect::<Room>(doc, *r)?;
                        let regions = housecraft_geom::rooms::detect(doc, rm.floor);
                        let i = regions.room_at(housecraft_geom::v2(rm.anchor)).ok_or_else(|| ApiError::Invalid("that room isn't enclosed".into()))?;
                        Some((rm.floor, regions.rooms[i].centerline.clone()))
                    }
                    None => None,
                };
                let ids: Vec<Id> = doc
                    .objects
                    .iter()
                    .filter(|(_, o)| kinds.contains(&o.kind()))
                    .map(|(id, _)| *id)
                    .filter(|id| floor.is_none_or(|f| doc.floor_of(*id) == Some(f)))
                    .filter(|id| match &inside {
                        None => true,
                        Some((f, poly)) => {
                            doc.floor_of(*id) == Some(*f)
                                && ops::position_of(doc, *id).is_some_and(|p| housecraft_geom::math::point_in_polygon(p, poly))
                        }
                    })
                    .collect();
                ops::delete(doc, &ids, &mut out)?;
            }
            Command::DeleteDimensions { floor, which } => {
                let ids: Vec<Id> = doc
                    .iter::<Dimension>()
                    .filter(|d| floor.is_none_or(|f| d.floor == f))
                    .filter(|d| match which {
                        DimensionsToDelete::Automatic => d.auto,
                        DimensionsToDelete::Manual => !d.auto,
                        DimensionsToDelete::All => true,
                    })
                    .map(|d| d.id)
                    .collect();
                ops::delete(doc, &ids, &mut out)?;
            }
            Command::ReverseWallLayers { id } => {
                let w = expect::<Wall>(doc, *id)?.clone();
                let len = housecraft_geom::v2(w.start).distance(housecraft_geom::v2(w.end));
                let flip = |d: Length| Length::from_mm(len - d.mm());
                for o in doc.openings_in(*id).cloned().collect::<Vec<_>>() {
                    if let Some(om) = doc.get_mut::<Opening>(o.id) {
                        om.position = flip(o.position);
                        // The same physical swing (the hinge is seen from that side).
                        om.swing_in = !o.swing_in;
                    }
                    out.modified.push(o.id);
                }
                if let Some(wm) = doc.get_mut::<Wall>(*id) {
                    std::mem::swap(&mut wm.start, &mut wm.end);
                    let s = &mut wm.shape;
                    for t in &mut s.top {
                        t.along = flip(t.along);
                    }
                    s.top.reverse();
                    for n in &mut s.niches {
                        n.position = flip(n.position);
                        n.interior = !n.interior;
                    }
                    for n in &mut s.newels {
                        *n = flip(*n);
                    }
                }
                out.modified.push(*id);
            }
            Command::AutoRoomDimension { room, offset } => {
                let rm = expect::<Room>(doc, *room)?.clone();
                let regions = housecraft_geom::rooms::detect(doc, rm.floor);
                let i = regions.room_at(housecraft_geom::v2(rm.anchor)).ok_or_else(|| ApiError::Invalid("that room isn't enclosed".into()))?;
                // Finished faces, counter-clockwise: the room is left of each edge.
                let poly = regions.rooms[i].polygon.clone();
                let n = poly.len();
                for k in 0..n {
                    let (a, b) = (poly[k], poly[(k + 1) % n]);
                    if a.distance(b) < 300.0 {
                        continue;
                    }
                    let pt = housecraft_geom::math::pt;
                    let dim = Command::AddDimension {
                        floor: Some(rm.floor),
                        layer: None,
                        geometry: DimensionGeometry::Linear { points: vec![pt(a), pt(b)], orientation: DimOrientation::Aligned, offset: *offset },
                        text_override: None,
                    }
                    .apply(doc)?;
                    out.merge(dim);
                }
            }
        }
        Ok(out)
    }
}

fn orbit_seconds() -> f64 {
    20.0
}

fn page_prefix() -> String {
    "A-".into()
}
