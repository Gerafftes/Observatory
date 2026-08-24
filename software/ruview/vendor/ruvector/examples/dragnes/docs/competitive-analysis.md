# DrAgnes Competitive Analysis

**Status**: Research & Planning
**Date**: 2026-03-21

## Market Overview

The AI dermatology market is projected to reach $2.8 billion by 2030 (CAGR ~22%). Key drivers include rising skin cancer incidence, dermatologist shortage (US faces a projected shortfall of 10,000+ dermatologists by 2035), and smartphone proliferation enabling mobile health.

The market is currently fragmented across consumer apps (SkinVision, Google), clinical platforms (MetaOptima, Canfield), and FDA-cleared devices (3Derm). No single platform combines collective learning, offline capability, dermoscopy-native design, and cryptographic provenance.

## Competitor Profiles

### 1. SkinVision

- **Type**: Consumer mobile app (iOS/Android)
- **Approach**: Smartphone camera photo (no dermoscopy)
- **AI Model**: Proprietary CNN (not disclosed)
- **Regulatory**: CE marked (EU Class IIa medical device), not FDA cleared
- **Pricing**: Subscription (approximately $10/month or $50/year)
- **Market**: Consumer direct, some B2B insurance partnerships
- **Data**: 6M+ photos analyzed (claimed)

**Strengths**:
- Large consumer user base
- Simple UX (point and shoot)
- Insurance partnerships (Netherlands, Australia)
- CE marking provides regulatory credibility

**Weaknesses**:
- No dermoscopy support (clinical photo only, significantly lower accuracy)
- Static model (does not learn from use)
- Consumer-grade (not positioned for clinical workflow)
- No EHR integration
- Privacy model unclear (images uploaded to cloud)
- No collective learning across users
- Sensitivity for melanoma: approximately 80-85% (vs. >95% target for DrAgnes with dermoscopy)

### 2. MoleMap

- **Type**: Clinical skin mapping service (clinics + teledermatology)
- **Approach**: Whole-body photography + dermatoscopy at dedicated clinics
- **AI Model**: AI-assisted triage (details not public)
- **Regulatory**: Clinical service (not a standalone device)
- **Pricing**: $300-600 per full-body mapping session
- **Market**: Australia, New Zealand, UK, Ireland
- **Coverage**: 40+ clinics across ANZ

**Strengths**:
- Established clinical brand (20+ years)
- Whole-body photography with longitudinal tracking
- Dermatologist review of every case
- Strong in high-incidence regions (Australia, New Zealand)

**Weaknesses**:
- Requires physical clinic visit (not mobile)
- Expensive per session
- Limited geographic coverage
- AI is assistive only, not well-documented
- No offline capability
- Proprietary closed ecosystem
- No collective learning across clinics

### 3. MetaOptima / DermEngine

- **Type**: Clinical AI platform for dermatologists
- **Approach**: Cloud-based dermoscopic image analysis + teledermatology
- **AI Model**: Deep learning classifiers (multiple architectures)
- **Regulatory**: Health Canada Class II, CE marked, not FDA cleared (as of 2026)
- **Pricing**: SaaS subscription (approximately $200-500/month per practice)
- **Market**: Canada, EU, expanding to US
- **Features**: Total body photography, lesion tracking, AI classification, teledermatology

**Strengths**:
- Comprehensive clinical platform
- Total body photography with AI-powered lesion tracking
- Teledermatology workflow
- EHR integration (select systems)
- Strong in Canada

**Weaknesses**:
- Cloud-dependent (no offline capability)
- No FDA clearance for US market
- Static models (periodic retraining, not continuous learning)
- No collective learning across practices
- No cryptographic provenance
- No WASM browser inference
- Privacy relies on standard cloud security (no differential privacy)

### 4. Canfield Scientific

- **Type**: Medical imaging systems (hardware + software)
- **Approach**: Professional-grade imaging equipment + IntelliStudio software
- **Products**: VEOS (dermoscopy), VECTRA (3D body mapping), IntelliStudio (AI analysis)
- **Regulatory**: FDA cleared (imaging systems, not AI classification)
- **Pricing**: Hardware $10,000-50,000+ per system; software subscription additional
- **Market**: Academic medical centers, high-end dermatology practices

**Strengths**:
- Gold-standard imaging quality
- 3D body mapping (VECTRA WB360)
- Established in research/academic settings
- Strong clinical validation literature
- FDA-cleared imaging hardware

**Weaknesses**:
- Extremely expensive (inaccessible to primary care)
- Hardware-dependent (no mobile/portable option)
- AI capabilities lagging behind pure-AI companies
- No collective learning
- No offline AI inference
- Proprietary ecosystem (vendor lock-in)

### 5. Google Health Dermatology AI

- **Type**: Research project / potential product
- **Approach**: Smartphone clinical photos (Google Lens integration)
- **AI Model**: Deep learning on large proprietary datasets (Nature Medicine 2020 publication)
- **Regulatory**: Not FDA cleared. Labeled as "information only" in Google Search
- **Pricing**: Free (integrated into Google Search/Lens)
- **Market**: Global consumer (billions of Google users)

**Strengths**:
- Massive distribution (Google Search/Lens)
- Enormous training datasets (Google scale)
- Strong research team (published in Nature Medicine)
- Free to end users
- Multilingual support

**Weaknesses**:
- Not a medical device (no regulatory clearance, no clinical use)
- Clinical photo only (no dermoscopy)
- Consumer-grade accuracy (sensitivity ~80% for melanoma in initial studies)
- No clinician workflow integration
- Privacy concerns (Google data practices)
- No offline capability
- No collective learning (Google learns, but users do not benefit from each other)
- No provenance or auditability
- Cannot be used for clinical decision-making

### 6. 3Derm (Fotodigm Inc.)

- **Type**: FDA-cleared AI for skin cancer detection
- **Approach**: Smartphone-based image capture with AI classification
- **AI Model**: CNN-based classification
- **Regulatory**: **FDA 510(k) cleared** (DEN200069, September 2021) -- one of the first
- **Pricing**: Not public (enterprise sales)
- **Market**: US clinical settings
- **Clearance**: "Aid in detecting skin cancer and other skin conditions in patients"

**Strengths**:
- **FDA cleared** (critical competitive advantage)
- Established regulatory pathway (predicate device for future submissions)
- Clinical positioning (for healthcare professionals)
- First-mover in FDA-cleared AI dermatology

**Weaknesses**:
- Limited to clinical photography (no dermoscopy integration documented)
- Small market presence
- No collective learning
- No offline capability
- Limited public information on accuracy metrics
- No provenance/witness chain

### 7. Mela Sciences / MelaFind (STRATA Skin Sciences)

- **Type**: FDA-cleared multispectral analysis device
- **Approach**: Dedicated hardware device with multispectral imaging (10 wavelengths)
- **Regulatory**: FDA PMA approved (2011) -- Class III
- **Status**: Commercially underperformed; STRATA pivoted to psoriasis/vitiligo treatment
- **Pricing**: $7,500 device + $150/use disposable

**Strengths**:
- First FDA PMA-approved AI skin lesion analyzer
- Multispectral imaging (beyond visible light)
- High sensitivity (>95%) in clinical trials

**Weaknesses**:
- Commercial failure (too expensive, complex workflow)
- Dedicated hardware (not mobile)
- Discontinued/de-emphasized by STRATA
- No learning capability
- Per-use consumable cost ($150) unsustainable

**Lesson for DrAgnes**: MelaFind proves that accuracy alone is insufficient. Workflow integration, cost, and usability are equally critical. DrAgnes must be easy, affordable, and mobile.

## Competitive Matrix

| Feature | DrAgnes | SkinVision | MoleMap | MetaOptima | Canfield | Google Health | 3Derm |
|---------|---------|-----------|---------|-----------|---------|--------------|-------|
| Dermoscopy support | Native | No | Clinic only | Yes | Yes | No | No |
| Mobile/phone-based | Yes | Yes | No | Partial | No | Yes | Yes |
| Offline capable | Yes (WASM) | No | No | No | No | No | No |
| Continuous learning | Yes (Brain) | No | No | No | No | No | No |
| Cross-practice learning | Yes (Brain) | No | No | No | No | No | No |
| FDA cleared | Target 2028 | No | N/A | No | Imaging only | No | Yes |
| HIPAA compliant | Yes | N/A | N/A | Unclear | Yes | No | Yes |
| Cryptographic provenance | Yes (SHAKE-256) | No | No | No | No | No | No |
| Differential privacy | Yes (epsilon=1.0) | No | No | No | No | No | No |
| EHR integration | Planned Phase 2 | No | No | Select | Select | No | Unknown |
| Practice-adaptive | Yes (LoRA) | No | No | No | No | No | No |
| Open architecture | Yes | No | No | No | No | No | No |
| Whole-body mapping | Planned Phase 2 | No | Yes | Yes | Yes (VECTRA) | No | No |
| 7-point checklist auto | Yes | No | No | Yes | No | No | No |
| Cost to practice | Low (SaaS) | N/A (consumer) | High (per visit) | Medium (SaaS) | Very High | Free | Enterprise |
| Melanoma sensitivity | >95% target | ~80-85% | Expert-dependent | ~87-92% | N/A | ~80% | Not public |

## DrAgnes Unique Value Proposition

### What DrAgnes Does That Nobody Else Does

1. **Learns From Your Practice**: SONA MicroLoRA adapts the base model to your patient population. A practice in equatorial Nigeria seeing high rates of acral melanoma gets a model tuned for that distribution. A Scandinavian practice seeing mostly fair-skinned patients with superficial spreading melanoma gets a different adaptation. No competitor offers this.

2. **Learns From Everyone (Privately)**: The pi.ruv.io brain aggregates de-identified knowledge from all participating practices. This is not federated learning (which averages models) -- this is knowledge graph enrichment where each diagnosis strengthens connections in a semantic graph. The knowledge is richer than any single model.

3. **Runs Offline**: The WASM-compiled CNN runs entirely in the browser. No internet, no cloud, no latency. Classify a lesion on a hiking trail, in a rural clinic with no connectivity, or in a disaster zone. No competitor can do this.

4. **Cryptographic Provenance**: Every classification carries a SHAKE-256 witness chain proving which model version, brain state, and input produced it. For FDA audits, malpractice defense, and clinical governance, this is invaluable. No competitor offers this.

5. **DermLite-Native**: Built specifically for dermoscopic imaging. The preprocessing pipeline, ABCDE automation, and pattern analysis are designed for DermLite's optical characteristics. Consumer apps working from phone photos cannot match dermoscopic accuracy.

6. **Open Architecture**: Built on open-source RuVector crates. Practices own their data. The model architecture is transparent. Research institutions can validate, extend, and contribute. Vendor lock-in is eliminated.

### Positioning Statement

**For dermatologists and primary care physicians** who need accurate, trustworthy skin lesion classification at the point of care, **DrAgnes is an AI-powered dermatology intelligence platform** that continuously learns from every participating practice while keeping patient data private. **Unlike** SkinVision (consumer app, no dermoscopy), MetaOptima (cloud-dependent, static model), and Canfield (expensive hardware), **DrAgnes** combines DermLite-native dermoscopic analysis with collective brain intelligence, offline WASM inference, and cryptographic provenance to deliver a system that gets smarter with every use and can be trusted in clinical settings.

## Market Entry Strategy

### Phase 1: Academic Pilot (2026-2027)
- Partner with 3-5 academic dermatology departments
- Publish validation studies comparing DrAgnes to existing tools
- Establish clinical evidence for FDA submission
- Target: JAMA Dermatology, British Journal of Dermatology publications

### Phase 2: FDA Clearance + Early Adopters (2027-2028)
- 510(k) submission with 3Derm as predicate
- Launch with 50 early-adopter dermatology practices
- SaaS pricing: $99-199/month/practice (low barrier)
- DermLite partnership for bundled sales

### Phase 3: Primary Care Expansion (2028-2030)
- Teledermatology workflow for PCP-to-dermatologist referral
- Integration with major EHR systems
- Target: primary care practices in dermatologist-shortage areas
- Insurance reimbursement partnerships

### Phase 4: Global Expansion (2030+)
- CE marking for EU market
- Regional brain instances for data sovereignty
- Multilingual support
- Partnerships with global health organizations for underserved populations
