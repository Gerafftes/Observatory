#!/usr/bin/env node
// Historical crawl evolutionary analysis
// Queries brain for temporal medical content and computes drift metrics
// ADR-119 implementation

const BRAIN_URL = process.env.BRAIN_URL || 'https://pi.ruv.io';
const AUTH = 'Bearer ruvector-crawl-2026';
const fs = require('fs');
const path = require('path');

async function fetchBrain(urlPath) {
  const res = await fetch(`${BRAIN_URL}${urlPath}`, {
    headers: { 'Authorization': AUTH }
  });
  if (!res.ok) throw new Error(`${urlPath}: ${res.status}`);
  return res.json();
}

async function searchMedical(query, limit = 50) {
  return fetchBrain(`/v1/memories/search?q=${encodeURIComponent(query)}&limit=${limit}`);
}

function cosineSim(a, b) {
  if (!a || !b || a.length !== b.length) return 0;
  let dot = 0, magA = 0, magB = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    magA += a[i] * a[i];
    magB += b[i] * b[i];
  }
  return dot / (Math.sqrt(magA) * Math.sqrt(magB) || 1);
}

async function main() {
  console.log('=== Historical Crawl Evolutionary Analysis ===\n');

  // Search for medical content across domains
  const domains = ['dermatology', 'melanoma', 'skin cancer', 'dermoscopy', 'basal cell carcinoma'];
  const allMemories = [];

  for (const domain of domains) {
    try {
      const results = await searchMedical(domain, 20);
      const memories = Array.isArray(results) ? results : results.memories || [];
      allMemories.push(...memories);
      console.log(`  ${domain}: ${memories.length} results`);
    } catch (err) {
      console.log(`  ${domain}: error - ${err.message}`);
    }
  }

  // Deduplicate by ID
  const seen = new Set();
  const unique = allMemories.filter(m => {
    if (seen.has(m.id)) return false;
    seen.add(m.id);
    return true;
  });

  console.log(`\nTotal unique memories: ${unique.length}`);

  // Analyze by creation date
  const byMonth = {};
  for (const m of unique) {
    const month = (m.created_at || '').slice(0, 7); // YYYY-MM
    if (!byMonth[month]) byMonth[month] = [];
    byMonth[month].push(m);
  }

  // Compute embedding similarity matrix for drift detection
  const embeddings = unique.filter(m => m.embedding && m.embedding.length > 0);
  const driftPairs = [];

  for (let i = 0; i < Math.min(embeddings.length, 50); i++) {
    for (let j = i + 1; j < Math.min(embeddings.length, 50); j++) {
      const sim = cosineSim(embeddings[i].embedding, embeddings[j].embedding);
      if (sim > 0.7) {
        driftPairs.push({
          a: embeddings[i].title,
          b: embeddings[j].title,
          similarity: sim,
          aDate: embeddings[i].created_at,
          bDate: embeddings[j].created_at,
        });
      }
    }
  }

  driftPairs.sort((a, b) => b.similarity - a.similarity);

  // Generate report
  let report = `# Historical Crawl Evolutionary Analysis\n\n`;
  report += `**Date**: ${new Date().toISOString().slice(0, 10)}\n`;
  report += `**Memories analyzed**: ${unique.length}\n`;
  report += `**Embedding pairs computed**: ${driftPairs.length}\n\n`;

  report += `## Knowledge Distribution by Month\n\n`;
  report += `| Month | Memories | Topics |\n|-------|----------|--------|\n`;
  for (const [month, mems] of Object.entries(byMonth).sort()) {
    const topics = [...new Set(mems.flatMap(m => (m.tags || []).slice(0, 3)))].slice(0, 5).join(', ');
    report += `| ${month} | ${mems.length} | ${topics} |\n`;
  }

  report += `\n## Most Similar Content Pairs (Potential Temporal Versions)\n\n`;
  report += `| Similarity | Content A | Content B |\n|-----------|-----------|----------|\n`;
  for (const pair of driftPairs.slice(0, 15)) {
    report += `| ${pair.similarity.toFixed(3)} | ${(pair.a || '?').slice(0, 40)} | ${(pair.b || '?').slice(0, 40)} |\n`;
  }

  report += `\n## Topic Clusters\n\n`;
  const tagCounts = {};
  for (const m of unique) {
    for (const tag of (m.tags || [])) {
      tagCounts[tag] = (tagCounts[tag] || 0) + 1;
    }
  }
  const topTags = Object.entries(tagCounts).sort((a, b) => b[1] - a[1]).slice(0, 20);
  report += `| Tag | Count |\n|-----|-------|\n`;
  for (const [tag, count] of topTags) {
    report += `| ${tag} | ${count} |\n`;
  }

  report += `\n## Key Findings\n\n`;
  report += `- Total medical knowledge memories: ${unique.length}\n`;
  report += `- High-similarity pairs (>0.7): ${driftPairs.length} (potential temporal versions or related content)\n`;
  report += `- Most common topic: ${topTags[0] ? topTags[0][0] : 'N/A'} (${topTags[0] ? topTags[0][1] : 0} memories)\n`;
  report += `- Date range: ${Object.keys(byMonth).sort()[0] || 'N/A'} to ${Object.keys(byMonth).sort().pop() || 'N/A'}\n`;

  // Write report
  const outDir = path.join(__dirname, '..', 'docs', 'research', 'DrAgnes');
  const outPath = path.join(outDir, 'evolution-analysis.md');
  fs.mkdirSync(outDir, { recursive: true });
  fs.writeFileSync(outPath, report);
  console.log(`\nReport written to: ${outPath}`);

  // Share summary to brain
  try {
    const shareRes = await fetch(`${BRAIN_URL}/v1/memories`, {
      method: 'POST',
      headers: { 'Authorization': AUTH, 'Content-Type': 'application/json' },
      body: JSON.stringify({
        title: `Evolutionary Analysis: ${unique.length} medical memories across ${Object.keys(byMonth).length} months`,
        content: `Historical crawl analysis found ${unique.length} unique medical memories with ${driftPairs.length} high-similarity pairs (potential temporal versions). Top topics: ${topTags.slice(0, 5).map(t => t[0]).join(', ')}. Date range: ${Object.keys(byMonth).sort()[0]} to ${Object.keys(byMonth).sort().pop()}.`,
        tags: ['evolution', 'historical-crawl', 'drift-analysis', 'medical', 'temporal'],
        category: 'pattern'
      })
    });
    if (shareRes.ok) console.log('Shared analysis to brain');
  } catch (err) {
    console.log('Brain share failed:', err.message);
  }
}

main().catch(console.error);
