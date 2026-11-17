// Varak PDF betiği: mutool run pdf.js <komut> <dosya>
var A4 = [0, 0, 595.28, 841.89];
var komut = scriptArgs[0], dosya = scriptArgs[1];

if (komut === "bilgi") print(new PDFDocument(dosya).countPages());
if (komut === "uygula") uygula(JSON.parse(read(dosya)));
if (komut === "ocr") ocr(JSON.parse(read(dosya)));
if (komut === "metinler") metinler(new PDFDocument(dosya));
if (komut === "numaralar") numaralar(JSON.parse(read(dosya)));
if (komut === "boyutlar") boyutlar(new PDFDocument(dosya));

// Her sayfanın döndürülmemiş genişliği, yüksekliği ve döndürme açısı: [[w, h, r], ...]
function boyutlar(doc) {
	var b = [];
	for (var i = 0; i < doc.countPages(); i++) {
		var p = doc.findPage(i), m = kutu(p);
		b.push([+m.get(2) - +m.get(0), +m.get(3) - +m.get(1), ((+p.get("Rotate") || 0) % 360 + 360) % 360]);
	}
	print(JSON.stringify(b));
}

// Her sayfanın mevcut metnini yazar: ["...", "", ...] (boş: metin yok)
function metinler(doc) {
	var m = [];
	for (var i = 0; i < doc.countPages(); i++) m.push(doc.loadPage(i).toStructuredText().asText());
	print(JSON.stringify(m));
}

// MuPDF, sayfada bulunmayan anahtarlar için null döndürür
function sozluk(o) { return o && o.isDictionary(); }
function kutu(sayfa) {
	var m = sayfa.get("CropBox");
	return m && m.isArray() ? m : sayfa.get("MediaBox");
}
function hex4(n) { return ("000" + n.toString(16)).slice(-4); }
function sayi(n) { return n.toFixed(2); }

// Taranmış PDF ya da görüntüye görünmez metin katmanı ekler. Orijinal sayfalar korunur.
// İstenirse sayfalar basılı numaralarına göre sıralanır. Sonunda raporu JSON olarak yazar.
function ocr(is) {
	var doc = new PDFDocument(), font = new Font("Arial", is.font), ox, oy;
	var veriler = is.tsvler.map(function (t, i) {
		var v = t ? tsvOku(read(t)) : null;
		return v && is.duzeltmeler && is.duzeltmeler[i] ? duzeltmeUygula(v, is.duzeltmeler[i]) : v;
	});
	var kaynak = is.resim ? null : new PDFDocument(is.kaynak);
	// OCR'dan atlanan (zaten metin içeren) sayfaların numarası mevcut metinden okunur
	var numaraVerisi = veriler.map(function (v, i) { return v || (kaynak && metinVerisi(kaynak.loadPage(i))); });
	var secilen = numaraSec(numaraVerisi.map(function (v) { return v ? adaylar(v) : []; }));
	var sira = veriler.map(function (v, i) { return i; }), siralandi = false, numaraYok = false;
	if (is.sira) sira = is.sira; // düzeltme sonrası: belirlenmiş sıra korunur
	else if (is.sirala !== "kapali" && !is.resim) {
		// Sayfaların en az yarısında numara okunabildiyse sıralanır
		if (secilen.filter(function (n) { return n; }).length * 2 >= secilen.length) {
			sira = siraBul(secilen);
			siralandi = true;
		} else numaraYok = true;
	}
	if (is.resim) {
		var img = new Image(is.kaynak);
		ox = 72 / (img.getXResolution() || 96); oy = 72 / (img.getYResolution() || 96);
		resimEkle(doc, is.kaynak, false);
	} else {
		var harita = doc.newGraftMap();
		sira.forEach(function (i) { harita.graftPage(-1, kaynak, i); });
		ox = oy = 72 / is.dpi;
	}
	// Yön algılamasıyla döndürülmesi gereken sayfalar (OCR bu yöne göre yapıldı)
	if (is.donus) sira.forEach(function (i, k) {
		if (!is.donus[i]) return;
		var p = doc.findPage(k);
		p.put("Rotate", ((+p.get("Rotate") || 0) + is.donus[i]) % 360);
	});
	var F = doc.addFont(font);
	sira.forEach(function (i, k) { if (veriler[i]) katmanEkle(doc, doc.findPage(k), veriler[i], font, F, ox, oy); });
	doc.save(is.cikti, "garbage,compress");
	var r = rapor(secilen, sira, numaraVerisi, siralandi, numaraYok);
	// Word ve EPUB için sayfa sayfa paragraflar
	r.paragraflar = sira.map(function (i) {
		return veriler[i] ? paragraflar(veriler[i]) : kaynak ? metinParagraflari(kaynak.loadPage(i).toStructuredText().asText()) : [];
	});
	if (is.sira) r.metin = sira.map(function (i) {
		return veriler[i] ? duzMetin(veriler[i]) : kaynak.loadPage(i).toStructuredText().asText();
	}).join("\n\f");
	print(JSON.stringify(r));
}

// Düzeltilen satırların kelimelerini, satırın tamamını kapsayan tek bir parçayla değiştirir.
// d: { "blok.paragraf.satir": "düzeltilmiş metin" }
function duzeltmeUygula(v, d) {
	var yeni = [], kutular = {};
	v.kelimeler.forEach(function (k) {
		if (typeof d[k.satir] !== "string") { yeni.push(k); return; }
		var b = kutular[k.satir];
		if (!b) {
			b = kutular[k.satir] = { x: k.x, y: k.y, s: k.x + k.w, a: k.y + k.h, guven: 100, satir: k.satir, metin: d[k.satir] };
			yeni.push(b);
		}
		b.x = Math.min(b.x, k.x); b.y = Math.min(b.y, k.y);
		b.s = Math.max(b.s, k.x + k.w); b.a = Math.max(b.a, k.y + k.h);
	});
	yeni.forEach(function (k) { if (k.s !== undefined) { k.w = k.s - k.x; k.h = k.a - k.y; } });
	return { H: v.H, kelimeler: yeni.filter(function (k) { return k.metin; }) };
}

// Arap harfleri Latin harflerinden çoksa paragraf sağdan sola yazılır
function harfYonu(s) {
	var arap = (s.match(/[\u0600-\u065F\u066E-\u06EF\u06FA-\u06FF\u0750-\u077F\uFB50-\uFDFF\uFE70-\uFEFF]/g) || []).length;
	var latin = (s.match(/[A-Za-z\u00C0-\u024F]/g) || []).length;
	return arap > latin;
}

// OCR verisini Tesseract'ın paragraf yapısına göre birleştirir; satır sonunda
// tireyle bölünmüş Latin harfli kelimeler birleştirilir.
function paragraflar(v) {
	var liste = [], son = null;
	v.kelimeler.forEach(function (k) {
		var anahtar = k.satir.split(".").slice(0, 2).join(".");
		if (anahtar !== son) { liste.push({ satirlar: [], satir: null }); son = anahtar; }
		var p = liste[liste.length - 1];
		if (k.satir !== p.satir) { p.satirlar.push([]); p.satir = k.satir; }
		p.satirlar[p.satirlar.length - 1].push(k.metin);
	});
	return liste.map(function (p) {
		var m = "";
		p.satirlar.forEach(function (s) {
			var t = s.join(" ");
			m = /[A-Za-z\u00C0-\u024F]-$/.test(m) ? m.slice(0, -1) + t : (m ? m + " " + t : t);
		});
		return { m: m, rtl: harfYonu(m) };
	});
}

// Zaten metin içeren (OCR'dan atlanan) sayfaların metnini boş satırlardan paragraflara ayırır
function metinParagraflari(t) {
	return t.split(/\n\s*\n/).map(function (p) { return p.replace(/\s+/g, " ").trim(); })
		.filter(function (p) { return p; })
		.map(function (p) { return { m: p, rtl: harfYonu(p) }; });
}

// Sayfa verisini satır satır düz metne çevirir
function duzMetin(v) {
	var satirlar = [], son = null;
	v.kelimeler.forEach(function (k) {
		if (k.satir !== son) { satirlar.push([]); son = k.satir; }
		satirlar[satirlar.length - 1].push(k.metin);
	});
	return satirlar.map(function (s) { return s.join(" "); }).join("\n");
}

// Sayfadaki mevcut metni, sayfa numarası aramak için TSV verisiyle aynı biçime çevirir
function metinVerisi(sayfa) {
	var b = sayfa.getBounds(), s = { H: b[3] - b[1], kelimeler: [] };
	JSON.parse(sayfa.toStructuredText().asJSON()).blocks.forEach(function (blok, bi) {
		(blok.lines || []).forEach(function (l, li) {
			l.text.split(/\s+/).forEach(function (w) {
				if (w) s.kelimeler.push({ x: l.bbox.x - b[0], y: l.bbox.y - b[1], w: l.bbox.w, h: l.bbox.h,
					guven: 100, satir: bi + "." + li, metin: w });
			});
		});
	});
	return s;
}

// PDF İşlemleri listesindeki sayfaları mevcut metindeki numaralara göre sıralar.
// Girdi: { sayfalar: [{ kaynak, no } | null] } — null: boş sayfa ya da görsel
function numaralar(is) {
	var belgeler = {};
	var veriler = is.sayfalar.map(function (s) {
		if (!s) return null;
		if (!belgeler[s.kaynak]) belgeler[s.kaynak] = new PDFDocument(s.kaynak);
		return metinVerisi(belgeler[s.kaynak].loadPage(s.no));
	});
	var secilen = numaraSec(veriler.map(function (v) { return v ? adaylar(v) : []; }));
	var sayili = secilen.filter(function (n) { return n; }).length;
	var siralandi = sayili * 2 >= veriler.filter(function (v) { return v; }).length && sayili > 0;
	var sira = siralandi ? siraBul(secilen) : secilen.map(function (n, i) { return i; });
	print(JSON.stringify(rapor(secilen, sira, veriler, siralandi, !siralandi)));
}

function tsvOku(tsv) {
	var s = { H: 0, kelimeler: [] };
	tsv.split("\n").forEach(function (r) {
		var c = r.split("\t"), metin = (c[11] || "").trim();
		if (c[0] === "1") s.H = +c[9];
		if (c[0] !== "5" || !metin) return;
		s.kelimeler.push({ x: +c[6], y: +c[7], w: +c[8], h: +c[9], guven: +c[10], satir: c[2] + "." + c[3] + "." + c[4], metin: metin });
	});
	return s;
}

// ---------- Sayfa numarası ----------

function roma(s) {
	var d = { i: 1, v: 5, x: 10, l: 50, c: 100, d: 500, m: 1000 }, t = 0;
	s = s.toLowerCase();
	for (var i = 0; i < s.length; i++) {
		var a = d[s[i]], b = d[s[i + 1]] || 0;
		t += a < b ? -a : a;
	}
	return t;
}

// Bir kelime sayfa numarası olabilir mi? Batı, Arap ve Fars rakamları ile
// (satırda tek başına duruyorsa) küçük Romen rakamları tanınır.
function sayiOku(metin, yalniz) {
	var t = metin.replace(/[\-–—()\[\]{}.,:;|'"«»*]/g, "")
		.replace(/[\u0660-\u0669]/g, function (c) { return c.charCodeAt(0) - 0x660; })
		.replace(/[\u06F0-\u06F9]/g, function (c) { return c.charCodeAt(0) - 0x6F0; });
	if (/^\d{1,4}$/.test(t)) return { deger: +t, roma: false, yazi: metin };
	if (yalniz && /^[ivxlcdm]{1,7}$/i.test(t)) {
		var v = roma(t);
		if (v > 0 && v <= 50) return { deger: v, roma: true, yazi: metin };
	}
	return null;
}

// Sayfanın üst ve alt %10'luk şeridindeki sayılar aday olur
function adaylar(s) {
	var satirda = {}, liste = [];
	s.kelimeler.forEach(function (k) { satirda[k.satir] = (satirda[k.satir] || 0) + 1; });
	s.kelimeler.forEach(function (k) {
		var uzaklik = Math.min(k.y, s.H - k.y - k.h);
		if (uzaklik > s.H * 0.1) return;
		var n = sayiOku(k.metin, satirda[k.satir] === 1);
		if (n) { n.uzaklik = uzaklik; liste.push(n); }
	});
	return liste;
}

// Her sayfa için en olası numarayı seçer. Üç ya da daha fazla sayfada aynı
// görünen değerler (yıl, cilt no, dipnot 1 gibi) sayfa numarası sayılmaz;
// kalanlardan en az tekrar edeni ve sayfa kenarına en yakın olanı seçilir.
function numaraSec(listeler) {
	var frekans = {};
	var anahtar = function (n) { return (n.roma ? "r" : "") + n.deger; };
	listeler.forEach(function (l) {
		var gorulen = {};
		l.forEach(function (n) {
			if (gorulen[anahtar(n)]) return;
			gorulen[anahtar(n)] = true;
			frekans[anahtar(n)] = (frekans[anahtar(n)] || 0) + 1;
		});
	});
	return listeler.map(function (l) {
		var iyi = null;
		l.forEach(function (n) {
			var f = frekans[anahtar(n)];
			if (f >= 3) return;
			if (!iyi || f < iyi.f || (f === iyi.f && n.uzaklik < iyi.n.uzaklik)) iyi = { n: n, f: f };
		});
		return iyi ? iyi.n : null;
	});
}

// Romen rakamlı sayfalar önce, sonra numaralılar. Numarasız bir sayfa, kendinden
// önce taranmış son numaralı sayfanın arkasında kalır.
function siraBul(secilen) {
	var anahtar = [], son = [-1, 0], ek = 0;
	secilen.forEach(function (n, i) {
		if (n) { son = [n.roma ? 0 : 1, n.deger]; ek = 0; }
		else ek++;
		anahtar.push(son.concat([ek, i]));
	});
	return secilen.map(function (n, i) { return i; }).sort(function (a, b) {
		for (var k = 0; k < 4; k++) if (anahtar[a][k] !== anahtar[b][k]) return anahtar[a][k] - anahtar[b][k];
		return 0;
	});
}

function rapor(secilen, sira, veriler, siralandi, numaraYok) {
	var say = {}, degerler = [], tekrar = [], eksik = [];
	if (siralandi) {
		secilen.forEach(function (n) {
			if (!n || n.roma) return;
			say[n.deger] = (say[n.deger] || 0) + 1;
			degerler.push(n.deger);
		});
		Object.keys(say).forEach(function (d) { if (say[d] > 1) tekrar.push(+d); });
		if (degerler.length)
			for (var v = Math.min.apply(null, degerler); v <= Math.max.apply(null, degerler); v++)
				if (!say[v]) eksik.push(v);
	}
	var sayfalar = [], dusuk = [], numarasiz = 0;
	sira.forEach(function (i, k) {
		var n = secilen[i], v = veriler[i], sorun = null;
		if (siralandi && v) {
			if (!n) { sorun = "numara okunamadı"; numarasiz++; }
			else if (!n.roma && say[n.deger] > 1) sorun = "tekrar";
		}
		var g = null;
		if (v && v.kelimeler.length) {
			g = 0;
			v.kelimeler.forEach(function (w) { g += Math.max(w.guven, 0); });
			g = Math.round(g / v.kelimeler.length);
			if (g < 60) dusuk.push(k + 1);
		}
		sayfalar.push({ numara: n ? n.yazi : null, sorun: sorun, guven: g });
	});
	return {
		siralandi: siralandi, numara_yok: numaraYok, sira: sira, tekrar: tekrar,
		eksik: eksik.slice(0, 30), eksik_sayisi: eksik.length, numarasiz: numarasiz,
		dusuk_guven: dusuk, sayfalar: sayfalar
	};
}

// ---------- Metin katmanı ----------

// Arap ve Fars rakamları (U+0660–0669, U+06F0–06F9) Arapça metinde de soldan sağa
// yazıldığı için hariç tutulur; yalnızca rakamdan oluşan kelimeler ters çevrilmez.
function sagdanSola(s) { return /[\u0590-\u065F\u066A-\u06EF\u06FA-\u08FF\uFB1D-\uFEFC]/.test(s); }

// Sağdan sola kelimelerin harfleri görsel sırayla (soldan sağa) yazılır; PDF
// okuyucuları kopyalarken ve ararken bunu doğru okuma sırasına çevirir.
function katmanEkle(doc, sayfa, veri, font, F, ox, oy) {
	var m = kutu(sayfa), x0 = +m.get(0), y0 = +m.get(1), x1 = +m.get(2), y1 = +m.get(3);
	// OCR görüntüsü sayfanın ekranda göründüğü hâlinden (döndürülmüş) üretilir.
	// nokta(u, v): ekrandaki noktayı (sol üstten, punto) sayfa koordinatına çevirir.
	var r = ((+sayfa.get("Rotate") || 0) % 360 + 360) % 360;
	function nokta(u, v) {
		if (r === 90) return [x0 + v, y0 + u];
		if (r === 180) return [x1 - u, y0 + v];
		if (r === 270) return [x1 - v, y1 - u];
		return [x0 + u, y1 - v];
	}
	var yon = { 90: "0 1 -1 0", 180: "-1 0 0 -1", 270: "0 -1 1 0" }[r] || "1 0 0 1";
	var icerik = ["Q", "BT 3 Tr"];
	veri.kelimeler.forEach(function (k) {
		var p = nokta(k.x * ox, (k.y + k.h) * oy), w = k.w * ox, h = k.h * oy;
		var harfler = k.metin.split("");
		if (sagdanSola(k.metin)) harfler.reverse();
		var gid = "", genislik = 0;
		harfler.forEach(function (c) {
			var g = font.encodeCharacter(c.charCodeAt(0));
			gid += hex4(g);
			genislik += font.advanceGlyph(g, 0);
		});
		if (!(genislik > 0 && h > 0)) return;
		icerik.push("/VarakF " + sayi(h) + " Tf " + sayi(100 * w / (genislik * h)) + " Tz " + yon + " " + sayi(p[0]) + " " + sayi(p[1]) + " Tm <" + gid + "> Tj");
	});
	icerik.push("ET");

	var kaynaklar = sayfa.get("Resources");
	if (!sozluk(kaynaklar)) { kaynaklar = doc.newDictionary(); sayfa.put("Resources", kaynaklar); }
	var fontlar = kaynaklar.get("Font");
	if (!sozluk(fontlar)) { fontlar = doc.newDictionary(); kaynaklar.put("Font", fontlar); }
	fontlar.put("VarakF", F);

	// Orijinal içerik q ... Q arasına alınır ki metin katmanının konumu bozulmasın
	var eski = sayfa.get("Contents"), yeni = doc.newArray();
	yeni.push(doc.addStream("q", {}));
	if (eski && eski.isArray()) for (var i = 0; i < eski.length; i++) yeni.push(eski.get(i));
	else if (eski && !eski.isNull()) yeni.push(eski);
	yeni.push(doc.addStream(icerik.join("\n"), {}));
	sayfa.put("Contents", yeni);
}

// ---------- PDF İşlemleri ----------

function uygula(plan) {
	var kaynaklar = {};
	plan.ciktilar.forEach(function (c) {
		var doc = new PDFDocument(), haritalar = {};
		c.sayfalar.forEach(function (s) {
			var n = doc.countPages();
			if (s.bos) doc.insertPage(-1, doc.addPage(A4, 0, {}, ""));
			else if (s.resim) resimEkle(doc, s.resim, plan.a4);
			else {
				if (!kaynaklar[s.kaynak]) kaynaklar[s.kaynak] = new PDFDocument(s.kaynak);
				if (!haritalar[s.kaynak]) haritalar[s.kaynak] = doc.newGraftMap();
				haritalar[s.kaynak].graftPage(-1, kaynaklar[s.kaynak], s.no);
			}
			var sayfa = doc.findPage(n);
			if (s.dondur) sayfa.put("Rotate", ((+sayfa.get("Rotate") || 0) + s.dondur) % 360);
			if (s.kirp) kirp(sayfa, s.kirp);
		});
		doc.save(c.dosya, "garbage,compress");
	});
}

// k: sayfanın görünen hâlinde sol, üst, sağ, alt kenarlardan kırpılacak oranlar (0-1).
// Döndürülmüş sayfada görünen kenar, döndürülmemiş sayfanın başka bir kenarına denk gelir.
function kirp(sayfa, k) {
	var m = kutu(sayfa), x0 = +m.get(0), y0 = +m.get(1), x1 = +m.get(2), y1 = +m.get(3);
	var d = Math.round(((+sayfa.get("Rotate") || 0) % 360 + 360) % 360 / 90);
	var f = function (j) { return k[(j + d) % 4]; }; // döndürülmemiş sayfada 0: sol, 1: üst, 2: sağ, 3: alt
	sayfa.put("CropBox", [x0 + f(0) * (x1 - x0), y0 + f(3) * (y1 - y0), x1 - f(2) * (x1 - x0), y1 - f(1) * (y1 - y0)]);
}

function resimEkle(doc, yol, a4) {
	var img = new Image(yol);
	var w = img.getWidth() * 72 / (img.getXResolution() || 96);
	var h = img.getHeight() * 72 / (img.getYResolution() || 96);
	var kutu = [0, 0, w, h], x = 0, y = 0;
	if (a4) {
		var o = Math.min(A4[2] / w, A4[3] / h);
		w *= o; h *= o;
		x = (A4[2] - w) / 2; y = (A4[3] - h) / 2;
		kutu = A4;
	}
	var icerik = "q " + sayi(w) + " 0 0 " + sayi(h) + " " + sayi(x) + " " + sayi(y) + " cm /R Do Q";
	doc.insertPage(-1, doc.addPage(kutu, 0, { XObject: { R: doc.addImage(img) } }, icerik));
}
