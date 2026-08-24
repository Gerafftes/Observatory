# DrAgnes DermLite Integration Research

**Status**: Research & Planning
**Date**: 2026-03-21

## Overview

DermLite (manufactured by 3Gen Inc., San Juan Capistrano, CA) is the world's most widely used line of dermatoscopes. DrAgnes is designed as a DermLite-native platform, providing purpose-built integration with their device ecosystem for standardized dermoscopic imaging and analysis.

## DermLite Device Lineup

### DermLite HUD (Heads-Up Display)

- **Form Factor**: Standalone camera with built-in display and optics
- **Magnification**: 10x polarized
- **Illumination**: LED ring with polarization filter
- **Camera**: Built-in 12MP sensor, 1920x1080 capture
- **Connectivity**: Wi-Fi (image transfer), Bluetooth (metadata/control)
- **Unique Features**:
  - Hands-free operation (no phone attachment needed)
  - Built-in display shows magnified real-time view
  - Dual-mode: polarized and non-polarized switching
  - Internal storage for batch capture
- **DrAgnes Integration**: Wi-Fi direct for image transfer; Bluetooth for device control and metadata. Best suited for high-volume clinical environments.

### DermLite DL5

- **Form Factor**: Handheld dermatoscope with smartphone adapter
- **Magnification**: 10x, hybrid polarized/non-polarized (toggle)
- **Illumination**: 20 PigmentBoost LEDs + 4 polarized LEDs
- **Adapter**: Universal magnetic mount (MagnetiConnect)
- **Power**: Rechargeable lithium-ion, 4+ hours continuous use
- **Unique Features**:
  - PigmentBoost mode enhances pigmented structures
  - Hybrid mode allows instant switching without contact loss
  - Crystal-clear optics with minimal distortion
  - Compact enough for pocket carry
- **DrAgnes Integration**: Phone camera passthrough via adapter. Camera API captures at phone's native resolution. DL5's PigmentBoost mode is flagged in metadata for preprocessing calibration.

### DermLite DL4

- **Form Factor**: Compact pocket dermatoscope
- **Magnification**: 10x, polarized only
- **Illumination**: LED ring, polarized
- **Adapter**: Smartphone adapter available (MagnetiConnect)
- **Power**: Rechargeable or AA batteries
- **Unique Features**:
  - Most affordable DermLite model
  - Widely adopted in primary care
  - Lightweight (50g)
- **DrAgnes Integration**: Same phone camera passthrough as DL5. Lower-tier device but adequate for DrAgnes classification. Ideal for primary care adoption.

### DermLite DL200 Hybrid

- **Form Factor**: Handheld with contact/non-contact dual mode
- **Magnification**: 10x
- **Illumination**: Hybrid LED system
- **Contact Mode**: Immersion fluid or direct contact with glass plate
- **Non-Contact Mode**: Cross-polarized at distance
- **Adapter**: Magnetic smartphone mount
- **Unique Features**:
  - Contact mode reveals subsurface structures (vessels, deeper pigment)
  - Non-contact mode for mucosal surfaces, painful areas
  - Dual-mode in single device
- **DrAgnes Integration**: Contact mode detection via metadata or image analysis (presence of glass plate reflection). Different preprocessing paths for contact vs. non-contact images.

## Image Capture Integration

### MediaStream API (Browser-Based)

```
DrAgnes Camera Module
    │
    ├── navigator.mediaDevices.getUserMedia({
    │       video: {
    │           facingMode: 'environment',    // Rear camera (DermLite side)
    │           width: { ideal: 1920 },
    │           height: { ideal: 1080 },
    │           frameRate: { ideal: 30 },
    │           focusMode: 'manual',          // Lock focus for dermoscopy
    │           whiteBalanceMode: 'manual',   // Calibrated for DermLite LEDs
    │       }
    │   })
    │
    ├── Live Preview (Canvas)
    │       ├── Real-time focus quality indicator
    │       ├── Lesion centering guide (circle overlay)
    │       ├── Exposure warning (over/under)
    │       └── DermLite detection indicator
    │
    ├── Capture (requestVideoFrameCallback)
    │       ├── High-res still capture (max sensor resolution)
    │       ├── Multi-frame averaging (3 frames for noise reduction)
    │       └── Auto-rotation correction
    │
    └── Storage (IndexedDB)
            ├── Original capture (encrypted)
            ├── Preprocessed 224x224 tensor
            └── Metadata (device, timestamp, settings)
```

### DermLite Device Detection

DrAgnes auto-detects DermLite attachment through multiple signals:

1. **Image analysis**: DermLite images have characteristic features:
   - Circular field of view (dark corners from circular optics)
   - Consistent illumination pattern (LED ring)
   - Magnification level (10x produces distinctive scale)
   - Polarization artifacts (cross-polarized light produces specific color shifts)

2. **EXIF metadata**: Some DermLite-phone combinations include device info

3. **User confirmation**: Manual DermLite model selection in UI as fallback

### Image Quality Assessment

Before classification, DrAgnes assesses image quality:

```
Quality Assessment Pipeline
    │
    ├── Focus Quality (Laplacian variance)
    │       ├── Score < 100: "Blurry -- please refocus"
    │       ├── Score 100-500: "Acceptable"
    │       └── Score > 500: "Sharp"
    │
    ├── Exposure Check (histogram analysis)
    │       ├── Mean intensity < 50: "Underexposed"
    │       ├── Mean intensity > 200: "Overexposed"
    │       └── Dynamic range < 100: "Low contrast"
    │
    ├── Lesion Coverage (center ROI analysis)
    │       ├── Lesion < 10% of frame: "Too far -- zoom in"
    │       ├── Lesion > 90% of frame: "Too close -- zoom out"
    │       └── Lesion off-center: "Center the lesion"
    │
    ├── Hair Occlusion (line detection)
    │       ├── > 20% coverage: "Excessive hair -- consider removal"
    │       └── Software hair removal applied regardless
    │
    └── Artifact Detection
            ├── Bubble artifacts (contact dermoscopy)
            ├── Reflection artifacts (glass plate)
            └── Motion blur (movement during capture)
```

## Dermoscopic Analysis Modules

### ABCDE Criteria Automation

The ABCDE mnemonic is the most widely taught screening tool for melanoma detection.

**A - Asymmetry**:
```
Method: Divide lesion along two perpendicular axes of maximum symmetry
    │
    ├── Segmentation: Otsu thresholding + morphological cleanup
    ├── Axis detection: Principal Component Analysis on contour points
    ├── Mirror comparison: XOR of left/right and top/bottom halves
    ├── Scoring:
    │       ├── 0: Symmetric along both axes
    │       ├── 1: Asymmetric along one axis
    │       └── 2: Asymmetric along both axes
    └── Weight: 1.3x (highest discriminative power for melanoma)
```

**B - Border Irregularity**:
```
Method: Divide border into 8 equal segments, assess each
    │
    ├── Contour extraction: Canny edge detection on segmentation mask
    ├── Segment division: 8 equal arc-length segments from centroid
    ├── Irregularity metrics per segment:
    │       ├── Fractal dimension (box-counting method)
    │       ├── Curvature variation (second derivative of contour)
    │       └── Abrupt border cutoff (gradient magnitude at boundary)
    ├── Scoring: 0-8 (count of irregular segments)
    └── Weight: 0.1x per segment
```

**C - Color**:
```
Method: Count distinct colors present in lesion
    │
    ├── Color space: Convert to perceptually uniform CIELAB
    ├── Reference colors (6 clinically significant):
    │       ├── Light brown (tan)
    │       ├── Dark brown
    │       ├── Black
    │       ├── Red
    │       ├── Blue-gray
    │       └── White (regression)
    ├── Detection: K-means clustering (k=6) + distance to reference
    ├── Scoring: 1-6 (count of colors present)
    └── Weight: 0.5x
```

**D - Diameter**:
```
Method: Maximum diameter of lesion in mm
    │
    ├── Calibration: DermLite ruler overlay or known magnification (10x)
    ├── Measurement: Maximum Feret diameter of segmentation contour
    ├── Threshold: 6mm is the clinical cutoff
    ├── Note: Nodular melanomas can be < 6mm; size alone is insufficient
    └── Weight: Binary (>= 6mm adds to risk score)
```

**E - Evolution**:
```
Method: Compare current image to prior captures of same lesion
    │
    ├── Registration: Affine alignment using lesion contour landmarks
    ├── Change detection:
    │       ├── Area change (growth rate in mm^2/month)
    │       ├── Color change (new colors appearing)
    │       ├── Shape change (symmetry score delta)
    │       ├── Border change (irregularity score delta)
    │       └── New structures (dermoscopic features appearing/disappearing)
    ├── Scoring: Composite change score normalized to 0-1
    └── Note: Most powerful criterion but requires longitudinal data
```

### 7-Point Checklist (Argenziano Method)

A structured scoring system for dermoscopic evaluation:

| Criterion | Points | Detection Method |
|-----------|--------|-----------------|
| Atypical pigment network | 2 (major) | CNN feature detection on dermoscopic structures |
| Blue-whitish veil | 2 (major) | Color analysis in blue-gray spectrum + opacity detection |
| Atypical vascular pattern | 2 (major) | Red channel analysis + vessel topology extraction |
| Irregular streaks | 1 (minor) | Directional filter banks + radial analysis from center |
| Irregular dots/globules | 1 (minor) | Blob detection (LoG) + regularity analysis |
| Irregular blotches | 1 (minor) | Connected component analysis in dark regions |
| Regression structures | 1 (minor) | White scar-like areas + blue-gray peppering detection |

**Interpretation**: Total score >= 3 suggests melanoma. Sensitivity ~95%, specificity ~75% in clinical studies.

**DrAgnes Implementation**: Each criterion has a dedicated CNN sub-head trained on the Derm7pt dataset which provides expert annotations for all 7 criteria. The sub-heads share the MobileNetV3 backbone but have independent classification layers.

### Menzies Method

A simplified 2-step approach used in clinical practice:

**Step 1 - Negative Features (must be absent for melanoma)**:
- Point symmetry of pigmentation
- Single color presence

**Step 2 - Positive Features (at least one must be present for melanoma)**:
1. Blue-white veil
2. Multiple brown dots
3. Pseudopods
4. Radial streaming
5. Scar-like depigmentation
6. Peripheral black dots/globules
7. Multiple colors (5-6)
8. Multiple blue-gray dots
9. Broadened network

**DrAgnes Implementation**: Binary classifiers for each positive and negative feature. If both negative features are absent AND at least one positive feature is present, flag for melanoma consideration.

### Pattern Analysis (Advanced Dermoscopy)

Beyond ABCDE and checklists, DrAgnes performs pattern-level analysis:

**Global Patterns**:
| Pattern | Association | Detection |
|---------|------------|-----------|
| Reticular | Benign melanocytic | Network detection via Gabor filters |
| Globular | Benign melanocytic | Blob detection (LoG, DoG) |
| Homogeneous | Benign (blue nevus, dermatofibroma) | Variance analysis (low variance = homogeneous) |
| Starburst | Spitz nevus or melanoma | Radial streaks from center + symmetry |
| Multicomponent | Melanoma (multiple patterns) | Pattern diversity score (entropy) |
| Nonspecific | Various | Low confidence flag for expert review |

**Local Structures**:
| Structure | Clinical Significance | Detection Method |
|-----------|---------------------|-----------------|
| Pigment network | Regular=benign, irregular=suspicious | Gabor filter response + regularity metrics |
| Dots | Regular=benign, irregular=melanoma | LoG blob detection + spatial distribution analysis |
| Globules | Regular=benign, irregular=melanoma | Larger blob detection + shape analysis |
| Streaks | Radial=melanoma, regular=Spitz | Directional filter + radial pattern detection |
| Blue-white veil | Melanoma indicator | Color segmentation + opacity detection |
| Regression structures | Melanoma regression | White+blue-gray area detection |
| Vascular structures | Various (type-dependent) | Red channel + vessel topology |
| Milia-like cysts | Seborrheic keratosis | Bright spot detection with specific shape |
| Comedo-like openings | Seborrheic keratosis | Dark spot detection + shape analysis |
| Leaf-like structures | BCC | Edge structure detection + morphology |
| Large blue-gray ovoid nests | BCC | Connected component + color analysis |

## EHR Integration Research

### FHIR R4 Resources

DrAgnes maps to standard FHIR resources for EHR interoperability:

| DrAgnes Entity | FHIR Resource | Notes |
|---------------|---------------|-------|
| DermImage | Media | With bodySite coding (SNOMED CT) |
| LesionClassification | DiagnosticReport | observationResult references |
| ABCDE Scores | Observation | One per criterion, grouped |
| Clinician Feedback | ClinicalImpression | Links to DiagnosticReport |
| Biopsy Result | DiagnosticReport | histopathology category |
| Follow-Up | ServiceRequest | scheduled monitoring |

### Practice Management Systems

| System | Integration Method | Coverage |
|--------|-------------------|----------|
| Epic | Epic on FHIR (R4), CDS Hooks | ~38% US market |
| Cerner (Oracle Health) | FHIR R4 API | ~25% US market |
| athenahealth | athenaFlex (FHIR R4) | ~10% US market |
| Modernizing Medicine (EMA) | Proprietary API + FHIR | Dermatology specialty leader |
| Nextech | Proprietary API | Dermatology/plastic surgery focus |

**Priority Integration**: Modernizing Medicine's EMA (Electronic Medical Assistant) is the dominant EHR for dermatology practices. Integration with EMA should be a Phase 2 priority.

## Calibration & Quality Assurance

### Color Calibration

DermLite LEDs have a known color temperature (~4500K). DrAgnes calibrates:
1. Capture image of ColorChecker (X-Rite) chart through DermLite
2. Compute color correction matrix (3x3 affine in CIELAB)
3. Apply correction to all subsequent captures
4. Re-calibrate monthly or when device changes

### Magnification Calibration

1. Capture image of known-size reference (DermLite ruler or 1mm grid)
2. Compute pixels-per-mm at 10x magnification
3. Store calibration factor per device
4. Use for accurate diameter measurements (ABCDE "D" criterion)

### Inter-Device Consistency

Different DermLite models produce subtly different images. DrAgnes normalizes:
- **Color normalization**: Shades of Gray algorithm standardizes illumination
- **Magnification normalization**: Resize to consistent pixels-per-mm
- **Polarization normalization**: Separate processing paths for polarized vs. non-polarized
- **Contact artifact handling**: Detect and compensate for contact plate reflections

## DermLite SDK & API Research

### Current State (2026)

3Gen Inc. does not provide a public SDK for DermLite devices. Integration relies on:
- Phone camera passthrough (DermLite acts as optical adapter)
- Wi-Fi direct for HUD model image transfer
- Bluetooth for HUD model control
- EXIF metadata extraction where available

### Recommended API Strategy

1. **Phase 1**: Camera API integration (no DermLite SDK dependency)
   - Works with all DermLite models via phone camera
   - Auto-detect DermLite presence via image analysis
   - Manual device selection fallback

2. **Phase 2**: Partner with 3Gen for official SDK access
   - Direct device control (focus, illumination, capture)
   - Device serial number for calibration persistence
   - Firmware version tracking for compatibility

3. **Phase 3**: Co-develop next-gen DermLite with embedded AI
   - On-device CNN inference (edge deployment)
   - Built-in calibration reference
   - Direct brain connectivity
   - Real-time AR overlay with diagnostic guidance
